//! The runtime directory and both ends of a same-user Unix connection.
use ibis_core::atomic_file::open_private;
use nix::poll::{PollFd, PollFlags, poll};
use nix::sys::socket::{self, AddressFamily, SockFlag, SockType, UnixAddr};
use nix::unistd::{Uid, geteuid};
use std::fs::{self, File, TryLockError};
use std::io;
use std::net::Shutdown;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Clone)]
pub(super) struct Address(PathBuf);

impl Address {
    pub fn new(namespace: &str, runtime_root: Option<&Path>) -> io::Result<Self> {
        let path = |root: &Path, dir: &str| root.join(dir).join(format!("{namespace}.sock"));
        let socket = if cfg!(target_os = "linux") {
            let root = runtime_root
                .filter(|root| root.is_absolute())
                .ok_or(io::ErrorKind::NotFound)?;
            path(root, "ibis")
        } else {
            let dir = format!("ibis-{}", geteuid());
            let temp = std::env::temp_dir();
            let socket = path(runtime_root.unwrap_or(&temp), &dir);
            if socket.is_absolute() && UnixAddr::new(&socket).is_ok() {
                socket
            } else if runtime_root.is_none() {
                path(Path::new("/tmp"), &dir)
            } else {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        };
        UnixAddr::new(&socket)?;
        Ok(Self(socket))
    }

    pub fn runtime_dir(&self) -> &Path {
        // Address::new joins the socket name to an absolute runtime directory.
        self.0.parent().unwrap_or(&self.0)
    }

    #[cfg(target_os = "macos")]
    pub(super) fn socket_path(&self) -> &Path {
        &self.0
    }

    pub fn prepare(&self) -> io::Result<()> {
        let dir = self.runtime_dir();
        let parent = dir.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let root = fs::metadata(parent)?;
        // A private parent or a sticky system temp directory protects the
        // runtime directory's name from replacement by another user.
        if !root.is_dir()
            || !matches!(root.uid(), 0) && root.uid() != geteuid().as_raw()
            || root.mode() & 0o022 != 0 && root.mode() & 0o1000 == 0
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        match fs::DirBuilder::new().mode(0o700).create(dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(dir)?;
        if !metadata.is_dir() || metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o7777 != 0o700 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }

    pub fn lock(&self, kind: &str) -> io::Result<Option<File>> {
        let path = self.0.with_extension(kind);
        let lock = open_private(&path)?;
        let metadata = lock.metadata()?;
        if !metadata.is_file() || metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o077 != 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        match lock.try_lock() {
            Ok(()) => Ok(Some(lock)),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error),
        }
    }

    fn check_socket(&self) -> io::Result<()> {
        let metadata = fs::symlink_metadata(&self.0)?;
        if !metadata.file_type().is_socket() || metadata.uid() != geteuid().as_raw() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }

    pub fn connect(&self) -> io::Result<Stream> {
        self.check_socket()?;
        let address = UnixAddr::new(&self.0)?;
        // A saturated listen backlog must never stall a terminal indefinitely.
        #[cfg(target_os = "linux")]
        let flags = SockFlag::SOCK_CLOEXEC;
        #[cfg(target_os = "macos")]
        let flags = SockFlag::empty();
        let fd = socket::socket(AddressFamily::Unix, SockType::Stream, flags, None)?;
        #[cfg(target_os = "macos")]
        nix::fcntl::fcntl(&fd, nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC))?;
        let socket = UnixStream::from(fd);
        socket.set_nonblocking(true)?;
        socket::connect(socket.as_raw_fd(), &address)?;
        socket.set_nonblocking(false)?;
        check_uid(geteuid(), peer(&socket)?)?;
        Ok(Stream(socket))
    }

    /// The caller holds the lifetime lock. Only its own stale socket can be removed.
    pub fn listen(&self) -> io::Result<Listener> {
        match self.check_socket() {
            Ok(()) => fs::remove_file(&self.0)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let socket = UnixListener::bind(&self.0)?;
        let listener = Listener {
            socket,
            path: self.0.clone(),
        };
        fs::set_permissions(&self.0, fs::Permissions::from_mode(0o600))?;
        listener.socket.set_nonblocking(true)?;
        Ok(listener)
    }
}

pub(super) struct Listener {
    socket: UnixListener,
    path: PathBuf,
}

impl Listener {
    pub fn accept(&mut self, timeout: Duration) -> io::Result<Option<Stream>> {
        let timeout = nix::poll::PollTimeout::try_from(timeout).unwrap_or(nix::poll::PollTimeout::MAX);
        let mut ready = [PollFd::new(self.socket.as_fd(), PollFlags::POLLIN)];
        match poll(&mut ready, timeout) {
            Ok(0) | Err(nix::errno::Errno::EINTR) => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        match self.socket.accept() {
            Ok((socket, _)) => {
                socket.set_nonblocking(false)?;
                Ok(peer(&socket)
                    .and_then(|peer| check_uid(geteuid(), peer))
                    .ok()
                    .map(|()| Stream(socket)))
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(target_os = "linux")]
fn peer(socket: &UnixStream) -> io::Result<Uid> {
    use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
    Ok(Uid::from_raw(getsockopt(socket, PeerCredentials)?.uid()))
}

#[cfg(target_os = "macos")]
fn peer(socket: &UnixStream) -> io::Result<Uid> {
    Ok(nix::unistd::getpeereid(socket)?.0)
}

fn check_uid(expected: Uid, actual: Uid) -> io::Result<()> {
    if expected != actual {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

pub(super) struct Stream(UnixStream);

impl Stream {
    pub fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }
    pub fn stop_reading(&self) {
        let _ = self.0.shutdown(Shutdown::Read);
    }
    pub fn settle_on_interrupt(&self) -> io::Result<ibis_sources::adapter::ClientOperation> {
        let socket = self.0.try_clone()?;
        Ok(ibis_sources::adapter::ClientOperation::start(move || {
            let _ = socket.shutdown(Shutdown::Write);
        }))
    }
    pub fn set_timeouts(&mut self, read: Option<Duration>, write: Duration) -> io::Result<()> {
        self.0.set_read_timeout(read)?;
        self.0.set_write_timeout(Some(write))
    }
}

impl io::Read for Stream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}

impl io::Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) struct Process(Child);

impl Process {
    pub(super) fn spawn(program: &Path) -> io::Result<Self> {
        let mut command = Command::new(program);
        command
            .arg("daemon")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        ibis_sources::adapter::detach(&mut command);
        command.spawn().map(Self)
    }

    pub(super) fn has_exited(&mut self) -> io::Result<bool> {
        Ok(self.0.try_wait()?.is_some())
    }

    pub(super) fn reap(mut self) {
        // MCP can outlive the daemon it started, even while waiting on stdin.
        let _ = std::thread::Builder::new()
            .name("ibis-daemon-reap".into())
            .spawn(move || self.0.wait());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_different_peer_uid_is_refused() {
        assert!(check_uid(Uid::from_raw(1), Uid::from_raw(1)).is_ok());
        assert_eq!(
            check_uid(Uid::from_raw(1), Uid::from_raw(2)).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn a_public_socket_directory_is_refused() {
        let root = tempfile::tempdir_in("/tmp").unwrap();
        let address = Address::new("invented", Some(root.path())).unwrap();
        address.prepare().unwrap();
        let _lock = address.lock("engine").unwrap().unwrap();
        let _listener = address.listen().unwrap();
        fs::set_permissions(address.0.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
        let endpoint = super::super::Endpoint(address);
        let error = endpoint.connect(&std::env::current_exe().unwrap()).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }
}
