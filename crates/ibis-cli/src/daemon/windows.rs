//! Overlapped local named pipes. Only this module handles Win32 capabilities.
#![allow(unsafe_code)]

use ibis_core::atomic_file::open_private;
use ibis_core::contract::Fingerprint;
use std::fs::{self, File, TryLockError};
use std::future::{Future, poll_fn};
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::sync::{Arc, OnceLock};
use std::task::Poll;
use std::time::Duration;
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::watch;
use windows_sys::Win32::{
    Foundation::{ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_NOT_CONNECTED, HANDLE, LocalFree},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::SECURITY_IDENTIFICATION,
    System::{
        Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
        Threading::{
            CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS, GetCurrentProcess, OpenProcess, OpenProcessToken,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

fn runtime() -> io::Result<&'static Runtime> {
    static RUNTIME: OnceLock<io::Result<Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .worker_threads(1)
                .enable_io()
                .enable_time()
                .build()
        })
        .as_ref()
        .map_err(|error| io::Error::from(error.kind()))
}

#[derive(Clone)]
pub(super) struct Address {
    name: String,
    sid: String,
    locks: PathBuf,
}

impl Address {
    pub fn new(namespace: &str, cache: Option<&Path>) -> io::Result<Self> {
        let cache = cache.filter(|path| path.is_absolute()).ok_or(io::ErrorKind::NotFound)?;
        // SAFETY: GetCurrentProcess returns a borrowed pseudohandle valid for this process.
        let sid = sid(unsafe { GetCurrentProcess() })?;
        let user = Fingerprint::of_bytes(sid.as_bytes());
        let name = format!(r"\\.\pipe\ibis-{}-{namespace}", user.as_str());
        Ok(Self {
            name,
            sid,
            locks: cache.join("ibis").join(namespace),
        })
    }

    pub fn prepare(&self) -> io::Result<()> {
        fs::create_dir_all(&self.locks)
    }

    pub fn lock(&self, kind: &str) -> io::Result<Option<File>> {
        let lock = open_private(&self.locks.join(kind))?;
        match lock.try_lock() {
            Ok(()) => Ok(Some(lock)),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error),
        }
    }

    pub fn connect(&self) -> io::Result<Stream> {
        let _entered = runtime()?.enter();
        let pipe = ClientOptions::new()
            .security_qos_flags(SECURITY_IDENTIFICATION)
            .open(&self.name)
            .map_err(|error| match error.raw_os_error() {
                Some(code) if code == ERROR_PIPE_BUSY as i32 => io::Error::from(io::ErrorKind::WouldBlock),
                _ => error,
            })?;
        same_sid(&self.sid, &peer(pipe.as_raw_handle(), false)?)?;
        Ok(Stream::new(Pipe::Client(pipe)))
    }

    pub fn listen(&self) -> io::Result<Listener> {
        Ok(Listener {
            address: self.clone(),
            pending: self.instance(true)?,
        })
    }

    fn instance(&self, first: bool) -> io::Result<NamedPipeServer> {
        // Protected DACL with exactly one ACE. The SID came from the process token.
        let descriptor = wide(&format!("D:P(A;;GA;;;{})", self.sid));
        let mut security = ptr::null_mut();
        // SAFETY: the descriptor is NUL-terminated and the out pointer is writable.
        check(unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor.as_ptr(),
                SDDL_REVISION_1,
                &mut security,
                ptr::null_mut(),
            )
        })?;
        let security = Local(security);
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security.0,
            bInheritHandle: 0,
        };
        let _entered = runtime()?.enter();
        // SAFETY: the attributes and security descriptor remain valid through creation.
        unsafe {
            ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(&self.name, ptr::from_mut(&mut attributes).cast())
        }
    }
}

pub(super) struct Listener {
    address: Address,
    pending: NamedPipeServer,
}

impl Listener {
    pub fn accept(&mut self, timeout: Duration) -> io::Result<Option<Stream>> {
        let accepted = runtime()?.block_on(async { tokio::time::timeout(timeout, self.pending.connect()).await });
        match accepted {
            Err(_) => return Ok(None),
            Ok(Err(error))
                if error
                    .raw_os_error()
                    .is_some_and(|code| code == ERROR_NO_DATA as i32 || code == ERROR_PIPE_NOT_CONNECTED as i32) =>
            {
                self.pending = self.address.instance(false)?;
                return Ok(None);
            }
            Ok(result) => result?,
        }
        // Keep one instance open at all times, including while refusing a peer.
        let socket = std::mem::replace(&mut self.pending, self.address.instance(false)?);
        Ok(peer(socket.as_raw_handle(), true)
            .and_then(|sid| same_sid(&self.address.sid, &sid))
            .ok()
            .map(|()| Stream::new(Pipe::Server(socket))))
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

struct Local(*mut std::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        // SAFETY: this pointer was allocated by the corresponding Win32 conversion call.
        let _ = unsafe { LocalFree(self.0) };
    }
}

fn check(success: i32) -> io::Result<()> {
    if success == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn same_sid(expected: &str, actual: &str) -> io::Result<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(io::ErrorKind::PermissionDenied.into())
    }
}

fn peer(pipe: HANDLE, server_end: bool) -> io::Result<String> {
    let mut pid = 0;
    // SAFETY: the caller holds the pipe open and PID points to writable storage.
    check(unsafe {
        if server_end {
            GetNamedPipeClientProcessId(pipe, &mut pid)
        } else {
            GetNamedPipeServerProcessId(pipe, &mut pid)
        }
    })?;
    // SAFETY: the PID came from the kernel and only limited query access is requested.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful OpenProcess returned a fresh, noninheritable handle.
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    sid(process.as_raw_handle())
}

fn sid(process: HANDLE) -> io::Result<String> {
    let mut token = ptr::null_mut();
    // SAFETY: process is valid and token points to writable storage.
    check(unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) })?;
    // SAFETY: the successful call gave ownership of a new token handle.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut length = 0;
    // SAFETY: the zero-size probe writes only the required length.
    let _ = unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, ptr::null_mut(), 0, &mut length) };
    if !(size_of::<TOKEN_USER>() as u32..=4096).contains(&length) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut buffer = vec![0_usize; (length as usize).div_ceil(size_of::<usize>())];
    let mut written = 0;
    // SAFETY: the buffer is aligned for TOKEN_USER and holds at least length bytes.
    check(unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut written,
        )
    })?;
    if written < size_of::<TOKEN_USER>() as u32 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut string = ptr::null_mut();
    // SAFETY: success initialized TOKEN_USER and its SID is backed by this live buffer.
    check(unsafe { ConvertSidToStringSidW((*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid, &mut string) })?;
    let _allocation = Local(string.cast());
    let mut count = 0;
    // SAFETY: the SDK returns a NUL-terminated UTF-16 string on success.
    unsafe {
        while *string.add(count) != 0 {
            count += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(string, count)).map_err(io::Error::other)
    }
}

enum Pipe {
    Server(NamedPipeServer),
    Client(NamedPipeClient),
}

impl Pipe {
    async fn read(&self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            let result = match self {
                Self::Server(pipe) => {
                    pipe.readable().await?;
                    pipe.try_read(buffer)
                }
                Self::Client(pipe) => {
                    pipe.readable().await?;
                    pipe.try_read(buffer)
                }
            };
            if !result
                .as_ref()
                .is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock)
            {
                return result;
            }
        }
    }

    async fn write(&self, buffer: &[u8]) -> io::Result<usize> {
        loop {
            let result = match self {
                Self::Server(pipe) => {
                    pipe.writable().await?;
                    pipe.try_write(buffer)
                }
                Self::Client(pipe) => {
                    pipe.writable().await?;
                    pipe.try_write(buffer)
                }
            };
            if !result
                .as_ref()
                .is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock)
            {
                return result;
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct Stream {
    pipe: Arc<Pipe>,
    stop: watch::Sender<bool>,
    read: Option<Duration>,
    write: Duration,
}

impl Stream {
    fn new(pipe: Pipe) -> Self {
        Self {
            pipe: Arc::new(pipe),
            stop: watch::channel(false).0,
            read: None,
            write: super::WRITE_TIMEOUT,
        }
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(self.clone())
    }
    pub fn stop_reading(&self) {
        self.stop.send_replace(true);
    }
    pub fn set_timeouts(&mut self, read: Option<Duration>, write: Duration) -> io::Result<()> {
        self.read = read;
        self.write = write;
        Ok(())
    }
}

impl io::Read for Stream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let mut stopped = self.stop.subscribe();
        let result = runtime()?.block_on(async {
            if *stopped.borrow() {
                return Ok(0);
            }
            let read = async {
                let mut stopped = std::pin::pin!(stopped.changed());
                let mut reading = std::pin::pin!(self.pipe.read(bytes));
                poll_fn(|context| {
                    if stopped.as_mut().poll(context).is_ready() {
                        Poll::Ready(Ok(0))
                    } else {
                        reading.as_mut().poll(context)
                    }
                })
                .await
            };
            match self.read {
                Some(timeout) => tokio::time::timeout(timeout, read)
                    .await
                    .map_err(|_| io::ErrorKind::TimedOut)?,
                None => read.await,
            }
        });
        match result {
            Err(error)
                if error
                    .raw_os_error()
                    .is_some_and(|code| code == ERROR_BROKEN_PIPE as i32 || code == ERROR_NO_DATA as i32) =>
            {
                Ok(0)
            }
            result => result,
        }
    }
}

impl io::Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        runtime()?.block_on(async {
            tokio::time::timeout(self.write, self.pipe.write(bytes))
                .await
                .map_err(|_| io::ErrorKind::TimedOut)?
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn detach(command: &mut Command) {
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_different_peer_sid_is_refused() {
        assert!(same_sid("S-1-5-21-1", "S-1-5-21-1").is_ok());
        assert_eq!(
            same_sid("S-1-5-21-1", "S-1-5-21-2").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn the_pipe_accepts_the_current_sid_and_refuses_a_second_first_instance() {
        let root = tempfile::tempdir().unwrap();
        let name = Fingerprint::of_bytes(root.path().as_os_str().as_encoded_bytes());
        let address = Address::new(name.as_str(), Some(root.path())).unwrap();
        let mut listener = address.listen().unwrap();
        assert!(address.listen().is_err());
        let _client = address.connect().unwrap();
        assert!(listener.accept(Duration::from_secs(2)).unwrap().is_some());
    }
}
