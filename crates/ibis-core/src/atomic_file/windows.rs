use std::fs::File;
use std::io;
use std::mem::{size_of, size_of_val};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::{addr_of_mut, null_mut};
use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::{
    ACL, ACL_REVISION, AddAccessAllowedAceEx, GetTokenInformation, InitializeAcl, InitializeSecurityDescriptor,
    IsValidSid, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SetSecurityDescriptorControl,
    SetSecurityDescriptorDacl, SetSecurityDescriptorOwner, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateFileW, DELETE, FILE_ALL_ACCESS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_ALWAYS, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

fn succeeded(value: i32) -> io::Result<()> {
    (value != 0).then_some(()).ok_or_else(io::Error::last_os_error)
}

pub(super) fn open(path: &Path, exclusive: bool) -> io::Result<File> {
    let mut name: Vec<_> = path.as_os_str().encode_wide().collect();
    if name.is_empty() || name.contains(&0) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    name.push(0);
    let mut token = null_mut();
    // SAFETY: the process pseudo handle is borrowed; token is a writable output.
    succeeded(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) })?;
    // SAFETY: OpenProcessToken transferred this handle's sole close responsibility.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    // TOKEN_USER plus SECURITY_MAX_SID_SIZE, with pointer alignment.
    let mut user = vec![0usize; (size_of::<TOKEN_USER>() + 68).div_ceil(size_of::<usize>())];
    let mut required = 0;
    // SAFETY: the live aligned buffer has the advertised byte capacity.
    succeeded(unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            user.as_mut_ptr().cast(),
            size_of_val(user.as_slice()) as u32,
            &mut required,
        )
    })?;
    if (required as usize) < size_of::<TOKEN_USER>() || required as usize > size_of_val(user.as_slice()) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    // SAFETY: TokenUser initialized this structure; its SID borrows user.
    let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    // SAFETY: the SID belongs to the live TokenUser buffer.
    succeeded(unsafe { IsValidSid(sid) })?;
    // One FILE_ALL_ACCESS ACE with space for a maximum SID, DWORD-aligned.
    let mut storage = [0u32; 32];
    let acl = storage.as_mut_ptr().cast::<ACL>();
    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let sd = addr_of_mut!(descriptor).cast();
    // SAFETY: ACL and descriptor storage are aligned and have the stated sizes;
    // the validated SID and all backing allocations live through CreateFileW.
    unsafe {
        succeeded(InitializeAcl(acl, size_of_val(&storage) as u32, ACL_REVISION))?;
        succeeded(AddAccessAllowedAceEx(acl, ACL_REVISION, 0, FILE_ALL_ACCESS, sid))?;
        succeeded(InitializeSecurityDescriptor(sd, 1))?;
        succeeded(SetSecurityDescriptorOwner(sd, sid, 0))?;
        succeeded(SetSecurityDescriptorDacl(sd, 1, acl, 0))?;
        succeeded(SetSecurityDescriptorControl(sd, SE_DACL_PROTECTED, SE_DACL_PROTECTED))?;
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    // SAFETY: the terminated path and security allocations are live. Windows
    // ignores the descriptor for existing files, preserving their security.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE | if exclusive { DELETE } else { WRITE_OWNER | WRITE_DAC },
            FILE_SHARE_READ | FILE_SHARE_WRITE | if exclusive { FILE_SHARE_DELETE } else { 0 },
            &attributes,
            if exclusive { CREATE_NEW } else { OPEN_ALWAYS },
            FILE_FLAG_OPEN_REPARSE_POINT,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW transferred this file handle's sole close responsibility.
    Ok(unsafe { File::from_raw_handle(handle) })
}
