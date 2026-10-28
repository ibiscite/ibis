//! Win32 calls for the closed reader on Windows: the only unsafe code in
//! Ibis (decision 0006). Each call sits in a small wrapper with a SAFETY note.

use super::{ClosedError, Control, drive_path};
use std::ffi::{OsString, c_void};
use std::mem::{offset_of, size_of, size_of_val};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{addr_of_mut, null, null_mut};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
use windows_sys::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_REVISION, AddAccessAllowedAceEx, CONTAINER_INHERIT_ACE,
    DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetSecurityDescriptorControl, GetTokenInformation, InitializeAcl,
    InitializeSecurityDescriptor, IsValidAcl, IsValidSid, OBJECT_INHERIT_ACE, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR,
    SetSecurityDescriptorControl, SetSecurityDescriptorDacl, SetSecurityDescriptorOwner, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, CreateFileW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NOT_CONTENT_INDEXED,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, GetDiskFreeSpaceExW, GetDriveTypeW, GetFileAttributesW, GetFileInformationByHandle,
    GetFullPathNameW, GetLongPathNameW, GetTempPathW, INVALID_FILE_ATTRIBUTES, MoveFileExW, OPEN_EXISTING,
    READ_CONTROL, RemoveDirectoryW, SetFileAttributesW,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath};

// Reparse points, offline files and both kinds of cloud recall placeholder.
const NONLOCAL: u32 = 0x0044_1400;
const NATIVE: ClosedError = ClosedError::ProcessCheck;
const PATH_CAPACITY: usize = 32_768;

fn succeeded(value: i32) -> Result<(), ClosedError> {
    if value == 0 { Err(NATIVE) } else { Ok(()) }
}

struct Handle(HANDLE);
impl Handle {
    fn new(raw: HANDLE) -> Result<Self, ClosedError> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(NATIVE)
        } else {
            Ok(Self(raw))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns a valid, non-pseudo handle, closed once.
        unsafe { CloseHandle(self.0) };
    }
}

struct Descriptor(PSECURITY_DESCRIPTOR);
impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: GetSecurityInfo returns a LocalAlloc allocation (or null).
        unsafe { LocalFree(self.0) };
    }
}

fn wide(path: &Path) -> Result<Vec<u16>, ClosedError> {
    let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
    if value.is_empty() || value.len() >= PATH_CAPACITY || value.contains(&0) {
        return Err(NATIVE);
    }
    value.push(0);
    Ok(value)
}

// Check the raw spelling too: Path::components silently removes interior '.'.
fn checked_path(path: &Path) -> Result<Vec<u16>, ClosedError> {
    let value = wide(path)?;
    if !drive_path(path) {
        return Err(NATIVE);
    }
    for part in value[3..value.len() - 1]
        .split(|c| matches!(*c, 47 | 92))
        .filter(|p| !p.is_empty())
    {
        if part == [46]
            || part == [46, 46]
            || part.iter().any(|c| *c == 58 || *c < 32 || (127..=159).contains(c))
            || matches!(part.last(), Some(46 | 32))
        {
            return Err(NATIVE);
        }
    }
    Ok(value)
}

fn attributes(path: &[u16], control: &Control) -> Result<u32, ClosedError> {
    control.check()?;
    // SAFETY: all callers provide a live, NUL-terminated UTF-16 path.
    let attributes = unsafe { GetFileAttributesW(path.as_ptr()) };
    if attributes == INVALID_FILE_ATTRIBUTES || attributes & NONLOCAL != 0 {
        return Err(NATIVE);
    }
    Ok(attributes)
}

pub(super) fn local(path: &Path, control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    let value = checked_path(path)?;
    let mut current = vec![value[0], 58, 92, 0];
    control.check()?;
    // SAFETY: current is a drive root with a trailing slash and NUL. This
    // classification precedes every attribute probe, including the root.
    let kind = unsafe { GetDriveTypeW(current.as_ptr()) };
    // DRIVE_REMOVABLE, DRIVE_FIXED, DRIVE_CDROM, DRIVE_RAMDISK. Numeric
    // constants avoid enabling System_WindowsProgramming just for names.
    if !matches!(kind, 2 | 3 | 5 | 6) {
        return Err(NATIVE);
    }
    attributes(&current, control)?;
    for part in value[3..value.len() - 1]
        .split(|c| matches!(*c, 47 | 92))
        .filter(|p| !p.is_empty())
    {
        current.pop();
        if current.last() != Some(&92) {
            current.push(92);
        }
        current.extend_from_slice(part);
        current.push(0);
        // Reject an ancestor before attempting any probe beneath it.
        attributes(&current, control)?;
    }
    control.check()
}

fn zotero_image(name: &[u16]) -> bool {
    [b"zotero".as_slice(), b"zotero.exe".as_slice()].iter().any(|expected| {
        name.len() == expected.len()
            && name
                .iter()
                .zip(*expected)
                .all(|(a, b)| *a == u16::from(*b) || *a == u16::from(b.to_ascii_uppercase()))
    })
}

pub(super) fn closed(control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    // SAFETY: process snapshots require no pointers or process access rights.
    let snapshot = Handle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    control.check()?;
    // SAFETY: entry is initialized and advertises the exact structure size.
    succeeded(unsafe { Process32FirstW(snapshot.0, &mut entry) })?;
    let mut seen_self = false;
    let mut running = false;
    loop {
        control.check()?;
        seen_self |= entry.th32ProcessID == std::process::id();
        let len = entry.szExeFile.iter().position(|c| *c == 0).ok_or(NATIVE)?;
        running |= zotero_image(&entry.szExeFile[..len]);
        control.check()?;
        // SAFETY: snapshot is live and entry still has its required size.
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            // SAFETY: read immediately after the failed enumeration call.
            if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
                return Err(NATIVE);
            }
            break;
        }
    }
    control.check()?;
    if !seen_self {
        Err(NATIVE)
    } else if running {
        Err(ClosedError::Running)
    } else {
        Ok(())
    }
}

pub(super) fn space(path: &Path, needed: u64, control: &Control) -> Result<(), ClosedError> {
    local(path, control)?;
    let value = wide(path)?;
    let mut available = 0;
    control.check()?;
    // SAFETY: value is terminated; available is writable; unused outputs may be null.
    succeeded(unsafe { GetDiskFreeSpaceExW(value.as_ptr(), &mut available, null_mut(), null_mut()) })?;
    control.check()?;
    if available < needed {
        Err(ClosedError::NoSpace)
    } else {
        Ok(())
    }
}

struct User {
    storage: Vec<usize>,
}
impl User {
    fn sid(&self) -> PSID {
        // SAFETY: GetTokenInformation initialized a TOKEN_USER in this
        // pointer-aligned heap buffer; its embedded SID lives in that buffer.
        unsafe { (*self.storage.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }
}

fn user(control: &Control) -> Result<User, ClosedError> {
    control.check()?;
    // SAFETY: GetCurrentProcess returns a borrowed pseudo handle.
    let process = unsafe { GetCurrentProcess() };
    let mut raw = null_mut();
    control.check()?;
    // SAFETY: process is valid; raw is a writable handle output.
    succeeded(unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut raw) })?;
    let token = Handle::new(raw)?;
    // TOKEN_USER plus SECURITY_MAX_SID_SIZE, rounded up for pointer alignment.
    let mut user = User {
        storage: vec![0; (size_of::<TOKEN_USER>() + 68).div_ceil(size_of::<usize>())],
    };
    let mut required = 0;
    control.check()?;
    // SAFETY: the aligned buffer has the exact advertised byte capacity.
    succeeded(unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            user.storage.as_mut_ptr().cast(),
            size_of_val(user.storage.as_slice()) as u32,
            &mut required,
        )
    })?;
    if (required as usize) < size_of::<TOKEN_USER>() || required as usize > size_of_val(user.storage.as_slice()) {
        return Err(NATIVE);
    }
    control.check()?;
    // SAFETY: the SID came from successful TokenUser information and its
    // backing allocation remains live for every use of User::sid().
    succeeded(unsafe { IsValidSid(user.sid()) })?;
    Ok(user)
}

fn open(path: &Path, access: u32, control: &Control) -> Result<Handle, ClosedError> {
    let value = wide(path)?;
    control.check()?;
    // SAFETY: value is terminated. No inherited handle/security descriptor;
    // open the reparse point itself, including directories, with full sharing.
    Handle::new(unsafe {
        CreateFileW(
            value.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    })
}

pub(super) fn verify(path: &Path, protected: bool, control: &Control) -> Result<(), ClosedError> {
    local(path, control)?;
    let user = user(control)?;
    let handle = open(path, READ_CONTROL | FILE_READ_ATTRIBUTES, control)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    control.check()?;
    // SAFETY: handle permits attribute reads and info is a writable output.
    succeeded(unsafe { GetFileInformationByHandle(handle.0, &mut info) })?;
    if info.dwFileAttributes & NONLOCAL != 0 {
        return Err(NATIVE);
    }
    verify_acl(&handle, &user, protected, control)
}

fn verify_acl(handle: &Handle, user: &User, protected: bool, control: &Control) -> Result<(), ClosedError> {
    let mut owner = null_mut();
    let mut acl = null_mut();
    let mut raw = null_mut();
    control.check()?;
    // SAFETY: handle grants READ_CONTROL; all requested outputs are writable.
    // Owner/DACL pointers borrow the returned, LocalFree-owned descriptor.
    let status = unsafe {
        GetSecurityInfo(
            handle.0,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut raw,
        )
    };
    let descriptor = Descriptor(raw);
    if status != 0 || descriptor.0.is_null() || owner.is_null() || acl.is_null() {
        return Err(NATIVE);
    }
    control.check()?;
    // SAFETY: owner points into the live OS-returned security descriptor.
    succeeded(unsafe { IsValidSid(owner) })?;
    control.check()?;
    // SAFETY: both SIDs have been validated and their allocations remain live.
    succeeded(unsafe { EqualSid(owner, user.sid()) })?;
    let mut flags = 0;
    let mut revision = 0;
    control.check()?;
    // SAFETY: descriptor is live; flags and revision are writable outputs.
    succeeded(unsafe { GetSecurityDescriptorControl(descriptor.0, &mut flags, &mut revision) })?;
    if protected && flags & SE_DACL_PROTECTED == 0 {
        return Err(NATIVE);
    }
    control.check()?;
    // SAFETY: acl points into the live OS-returned descriptor.
    succeeded(unsafe { IsValidAcl(acl) })?;
    // SAFETY: the ACL header was validated and is backed by descriptor.
    if unsafe { (*acl).AceCount } != 1 {
        return Err(NATIVE);
    }
    let mut raw_ace = null_mut();
    control.check()?;
    // SAFETY: the valid ACL contains exactly one ACE and raw_ace is writable.
    succeeded(unsafe { GetAce(acl, 0, &mut raw_ace) })?;
    if raw_ace.is_null() {
        return Err(NATIVE);
    }
    // SAFETY: GetAce on a valid ACL returns an aligned, bounded ACE header.
    let header = unsafe { &*raw_ace.cast::<ACE_HEADER>() };
    let sid_offset = offset_of!(ACCESS_ALLOWED_ACE, SidStart);
    // ACCESS_ALLOWED_ACE_TYPE = 0. Include inherited and explicit ACEs alike;
    // .NET FullControl is exactly FILE_ALL_ACCESS, including SYNCHRONIZE.
    if header.AceType != 0 || (header.AceSize as usize) < sid_offset + 8 {
        return Err(NATIVE);
    }
    // SAFETY: the allow-ACE type and size above cover ACCESS_ALLOWED_ACE.
    let ace = unsafe { &*raw_ace.cast::<ACCESS_ALLOWED_ACE>() };
    if ace.Mask != FILE_ALL_ACCESS {
        return Err(NATIVE);
    }
    // SAFETY: the validated ACL bounds this ACE; the check above ensures a
    // complete SID header. Check subauthority length before calling SID APIs.
    let sid_bytes = unsafe {
        std::slice::from_raw_parts(
            raw_ace.cast::<u8>().add(sid_offset),
            ace.Header.AceSize as usize - sid_offset,
        )
    };
    if 8 + usize::from(sid_bytes[1]) * 4 > sid_bytes.len() {
        return Err(NATIVE);
    }
    let sid = sid_bytes.as_ptr().cast_mut().cast();
    control.check()?;
    // SAFETY: sid's header and subauthorities fit within this live ACE.
    succeeded(unsafe { IsValidSid(sid) })?;
    control.check()?;
    // SAFETY: both SIDs have been validated and their backing memory is live.
    succeeded(unsafe { EqualSid(sid, user.sid()) })?;
    control.check()
}

fn create_directory(path: &Path, user: &User, control: &Control) -> Result<(), ClosedError> {
    let value = wide(path)?;
    // More than ACL header + ACCESS_ALLOWED_ACE header + maximum SID (84 bytes).
    // u32 storage supplies the alignment required for ACL and ACE records.
    let mut storage = [0u32; 32];
    let acl = storage.as_mut_ptr().cast::<ACL>();
    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let sd = addr_of_mut!(descriptor).cast::<c_void>();
    control.check()?;
    // SAFETY: acl has 128 writable, DWORD-aligned bytes.
    succeeded(unsafe { InitializeAcl(acl, size_of_val(&storage) as u32, ACL_REVISION) })?;
    control.check()?;
    // SAFETY: acl is initialized and has room for this validated SID's ACE.
    succeeded(unsafe {
        AddAccessAllowedAceEx(
            acl,
            ACL_REVISION,
            CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE,
            FILE_ALL_ACCESS,
            user.sid(),
        )
    })?;
    control.check()?;
    // SAFETY: sd points to a writable, correctly aligned SECURITY_DESCRIPTOR.
    succeeded(unsafe { InitializeSecurityDescriptor(sd, 1) })?;
    control.check()?;
    // SAFETY: sd is initialized and user owns the live SID allocation.
    succeeded(unsafe { SetSecurityDescriptorOwner(sd, user.sid(), 0) })?;
    control.check()?;
    // SAFETY: sd and acl are initialized and remain live through creation.
    succeeded(unsafe { SetSecurityDescriptorDacl(sd, 1, acl, 0) })?;
    control.check()?;
    // SAFETY: sd is initialized; only the documented protection bit is set.
    succeeded(unsafe { SetSecurityDescriptorControl(sd, SE_DACL_PROTECTED, SE_DACL_PROTECTED) })?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    control.check()?;
    // SAFETY: the terminated path and descriptor/ACL/SID allocations remain
    // live throughout the call. CreateDirectoryW refuses an existing path.
    succeeded(unsafe { CreateDirectoryW(value.as_ptr(), &attributes) })
}

struct Staging(Option<Vec<u16>>);
impl Drop for Staging {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            // SAFETY: only a successful CreateDirectoryW arms this guard.
            // RemoveDirectoryW never recursively removes directory contents.
            unsafe { RemoveDirectoryW(path.as_ptr()) };
        }
    }
}

pub(super) fn create(path: &Path, control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    let target = checked_path(path)?;
    let parent = path.parent().ok_or(NATIVE)?;
    local(parent, control)?;
    control.check()?;
    // SAFETY: target is terminated; parent passed the locality gate first.
    if unsafe { GetFileAttributesW(target.as_ptr()) } != INVALID_FILE_ATTRIBUTES {
        return Err(NATIVE);
    }
    // SAFETY: read immediately after the failed attribute query. Other
    // failures (access denied, missing parent, etc.) do not prove absence.
    if unsafe { GetLastError() } != ERROR_FILE_NOT_FOUND {
        return Err(NATIVE);
    }
    let user = user(control)?;
    let mut random = [0; 16];
    control.check()?;
    // SAFETY: the system RNG needs no algorithm handle and fills this buffer.
    if unsafe {
        BCryptGenRandom(
            null_mut(),
            random.as_mut_ptr(),
            random.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    } < 0
    {
        return Err(NATIVE);
    }
    let staging_path = parent.join(format!("ibis-zotero-private-{:032x}", u128::from_le_bytes(random)));
    let staging_name = wide(&staging_path)?;
    create_directory(&staging_path, &user, control)?;
    let mut staging = Staging(Some(staging_name.clone()));
    verify(&staging_path, true, control)?;
    control.check()?;
    // SAFETY: both terminated paths share a local parent. Flags = 0 forbids
    // replacing an existing destination or copying across volumes.
    succeeded(unsafe { MoveFileExW(staging_name.as_ptr(), target.as_ptr(), 0) })?;
    staging.0 = None;
    // Only the staging name may be cleaned up on failure, never final data.
    verify(path, true, control)
}

fn normalized(path: &Path, control: &Control) -> Result<Vec<u16>, ClosedError> {
    let value = wide(path)?;
    let mut full = vec![0; PATH_CAPACITY];
    control.check()?;
    // SAFETY: input is terminated; output capacity is advertised accurately;
    // the optional filename output is unused. This call is lexical only.
    let len = unsafe { GetFullPathNameW(value.as_ptr(), full.len() as u32, full.as_mut_ptr(), null_mut()) } as usize;
    if len == 0 || len >= full.len() {
        return Err(NATIVE);
    }
    full.truncate(len + 1);
    // Long-name expansion touches the filesystem, so gate all ancestors first.
    local(Path::new(&OsString::from_wide(&full[..len])), control)?;
    let mut long = vec![0; PATH_CAPACITY];
    control.check()?;
    // SAFETY: full is terminated and long's advertised capacity is writable.
    let len = unsafe { GetLongPathNameW(full.as_ptr(), long.as_mut_ptr(), long.len() as u32) } as usize;
    if len == 0 || len >= long.len() {
        return Err(NATIVE);
    }
    long.truncate(len);
    while long.len() > 3 && matches!(long.last(), Some(47 | 92)) {
        long.pop();
    }
    Ok(long)
}

fn ordinal_equal(a: &[u16], b: &[u16], control: &Control) -> Result<bool, ClosedError> {
    control.check()?;
    // SAFETY: slices are live, nonempty and bounded by PATH_CAPACITY, fitting
    // i32. Explicit counts exclude terminators. TRUE requests ordinal folding.
    let result = unsafe { CompareStringOrdinal(a.as_ptr(), a.len() as i32, b.as_ptr(), b.len() as i32, 1) };
    if result == 0 {
        Err(NATIVE)
    } else {
        Ok(result == CSTR_EQUAL)
    }
}

struct KnownPath(*mut u16);
impl Drop for KnownPath {
    fn drop(&mut self) {
        // SAFETY: SHGetKnownFolderPath allocates with the COM task allocator;
        // CoTaskMemFree accepts null, including on a failed query.
        unsafe { CoTaskMemFree(self.0.cast()) };
    }
}

fn real_temp(control: &Control) -> Result<PathBuf, ClosedError> {
    let mut raw = null_mut();
    control.check()?;
    // SAFETY: the GUID and output are valid; null token uses the current
    // user. DONT_VERIFY avoids probing a redirected folder before locality.
    let status =
        unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DONT_VERIFY as u32, null_mut(), &mut raw) };
    let path = KnownPath(raw);
    if status < 0 || path.0.is_null() {
        return Err(NATIVE);
    }
    let mut len = 0;
    // SAFETY: a successful SHGetKnownFolderPath returns a NUL-terminated,
    // allocated UTF-16 string, kept alive by path while scanned and copied.
    while unsafe { *path.0.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: the scan established this initialized range in path's allocation.
    let value = unsafe { std::slice::from_raw_parts(path.0, len) };
    Ok(PathBuf::from(OsString::from_wide(value)).join("Temp"))
}

fn configured_temp(control: &Control) -> Result<PathBuf, ClosedError> {
    let mut value = vec![0; PATH_CAPACITY];
    control.check()?;
    // SAFETY: the advertised output buffer is writable. GetTempPathW returns
    // a configured string without checking whether its directory exists.
    let len = unsafe { GetTempPathW(value.len() as u32, value.as_mut_ptr()) } as usize;
    if len == 0 || len >= value.len() {
        return Err(NATIVE);
    }
    Ok(PathBuf::from(OsString::from_wide(&value[..len])))
}

pub(super) fn backup(path: &Path, control: &Control) -> Result<(), ClosedError> {
    local(path, control)?;
    let temp = normalized(&real_temp(control)?, control)?;
    let configured = normalized(&configured_temp(control)?, control)?;
    let expanded = normalized(path, control)?;
    if !ordinal_equal(&temp, &configured, control)? {
        return Err(NATIVE);
    }
    let mut prefix = temp;
    prefix.push(92);
    if expanded.len() <= prefix.len() || !ordinal_equal(&prefix, &expanded[..prefix.len()], control)? {
        return Err(NATIVE);
    }
    let value = wide(path)?;
    let attributes = attributes(&value, control)?;
    control.check()?;
    // SAFETY: path is terminated and local; preserve every existing attribute.
    succeeded(unsafe { SetFileAttributesW(value.as_ptr(), attributes | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED) })?;
    control.check()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::closed::tests::control;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic native directory"))
    }

    #[test]
    fn process_snapshot_is_complete_and_contains_our_pid() {
        // Success requires a nonempty snapshot, our PID, and normal end of
        // enumeration. Windows CI has no Zotero process.
        assert_eq!(super::super::closed(&control()), Ok(()));
    }

    #[test]
    fn only_exact_zotero_image_names_are_running() {
        for name in ["zotero", "ZOTERO.EXE", "ZoTeRo.exe"] {
            assert!(zotero_image(&name.encode_utf16().collect::<Vec<_>>()));
        }
        for name in [
            "zotero-bin",
            "zotero-mcp.exe",
            "ibis_zotero_local-test.exe",
            "Zotero.Beta.exe",
        ] {
            assert!(!zotero_image(&name.encode_utf16().collect::<Vec<_>>()));
        }
    }

    #[test]
    fn ambiguous_paths_and_remote_roots_are_refused() {
        for path in [
            r"C:relative",
            r"\\server\share\data",
            r"\\?\C:\data",
            r"C:\a\..\b",
            r"C:\a\.\b",
            r"C:\a:stream",
            r"C:\a.\b",
            r"C:\a \b",
            "C:\\a\u{1f}\\b",
        ] {
            assert_eq!(
                super::super::directory(Path::new(path), &control()).err(),
                Some(ClosedError::UnsafeSource)
            );
        }
    }

    #[test]
    fn create_private_succeeds_and_never_replaces_an_existing_path() {
        let parent = scratch();
        let path = parent.path().join("private");
        assert_eq!(super::super::create_private(&path, &control()), Ok(()));
        assert_eq!(super::super::secure(&path, &control()), Ok(()));
        std::fs::write(path.join("sentinel"), b"invented").unwrap_or_else(|_| panic!("synthetic sentinel"));
        assert_eq!(
            super::super::create_private(&path, &control()),
            Err(ClosedError::PrivateStorage)
        );
        assert_eq!(std::fs::read(path.join("sentinel")).unwrap_or_default(), b"invented");
        assert_eq!(
            std::fs::read_dir(parent.path())
                .unwrap_or_else(|_| panic!("synthetic listing"))
                .count(),
            1
        );
    }

    #[test]
    fn failed_creation_removes_only_its_staging_directory() {
        for cancel in [true, false] {
            let parent = scratch();
            let path = parent.path().join("private");
            let observed_parent = parent.path().to_owned();
            let destination = path.clone();
            let injected = std::sync::atomic::AtomicBool::new(false);
            let control = Control::new(std::sync::Arc::new(move || {
                let staged = std::fs::read_dir(&observed_parent)
                    .unwrap_or_else(|_| panic!("synthetic staging listing"))
                    .filter_map(Result::ok)
                    .any(|entry| entry.file_name().to_string_lossy().starts_with("ibis-zotero-private-"));
                if !staged {
                    return false;
                }
                if cancel {
                    return true;
                }
                if !injected.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    std::fs::create_dir(&destination).unwrap_or_else(|_| panic!("synthetic competing directory"));
                    std::fs::write(destination.join("sentinel"), b"invented")
                        .unwrap_or_else(|_| panic!("synthetic competing sentinel"));
                }
                false
            }));
            assert_eq!(
                super::super::create_private(&path, &control),
                Err(if cancel {
                    ClosedError::Cancelled
                } else {
                    ClosedError::PrivateStorage
                })
            );
            assert_eq!(
                std::fs::read_dir(parent.path())
                    .unwrap_or_else(|_| panic!("synthetic listing"))
                    .count(),
                usize::from(!cancel)
            );
            if cancel {
                assert!(!path.exists());
            } else {
                assert_eq!(std::fs::read(path.join("sentinel")).unwrap_or_default(), b"invented");
            }
        }
    }

    #[test]
    fn inherited_full_control_is_verified_but_is_not_protected() {
        let parent = scratch();
        let path = parent.path().join("private");
        assert_eq!(super::super::create_private(&path, &control()), Ok(()));
        let file = path.join("invented-file");
        std::fs::write(&file, b"invented").unwrap_or_else(|_| panic!("synthetic file"));
        assert_eq!(verify(&file, false, &control()), Ok(()));
        assert_eq!(
            super::super::secure(&file, &control()),
            Err(ClosedError::PrivateStorage)
        );
    }

    fn replace_acl(path: &Path, mask: u32, count: usize) {
        use windows_sys::Win32::Security::Authorization::SetSecurityInfo;
        use windows_sys::Win32::Security::PROTECTED_DACL_SECURITY_INFORMATION;
        use windows_sys::Win32::Storage::FileSystem::WRITE_DAC;
        let control = control();
        let user = user(&control).unwrap_or_else(|_| panic!("synthetic user"));
        let handle = open(path, WRITE_DAC, &control).unwrap_or_else(|_| panic!("synthetic ACL handle"));
        let mut storage = [0u32; 64];
        let acl = storage.as_mut_ptr().cast::<ACL>();
        // SAFETY: storage is DWORD-aligned and has the advertised capacity.
        assert_ne!(
            unsafe { InitializeAcl(acl, size_of_val(&storage) as u32, ACL_REVISION) },
            0
        );
        for _ in 0..count {
            // SAFETY: this initialized ACL has room for two maximum-sized
            // SID ACEs, and user owns the validated SID throughout the call.
            assert_ne!(
                unsafe { AddAccessAllowedAceEx(acl, ACL_REVISION, 0, mask, user.sid()) },
                0
            );
        }
        // SAFETY: handle grants WRITE_DAC; the initialized ACL is live and
        // owner/group/SACL are unused. Only this invented object's DACL changes.
        assert_eq!(
            unsafe {
                SetSecurityInfo(
                    handle.0,
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    acl,
                    null(),
                )
            },
            0
        );
    }

    #[test]
    fn empty_extra_and_partial_access_rules_are_refused() {
        let parent = scratch();
        let path = parent.path().join("private");
        assert_eq!(super::super::create_private(&path, &control()), Ok(()));
        for (mask, count) in [(FILE_ALL_ACCESS, 0), (FILE_ALL_ACCESS, 2), (FILE_ALL_ACCESS & !2, 1)] {
            replace_acl(&path, mask, count);
            assert_eq!(
                super::super::secure(&path, &control()),
                Err(ClosedError::PrivateStorage)
            );
        }
        replace_acl(&path, FILE_ALL_ACCESS, 1);
        assert_eq!(super::super::secure(&path, &control()), Ok(()));
    }

    // Native NTFS junctions need no symlink privilege or developer mode.
    // This keeps the reparse regression active on ordinary Windows CI users.
    fn junction(path: &Path, target: &Path) {
        use windows_sys::Win32::Foundation::GENERIC_WRITE;
        use windows_sys::Win32::System::IO::DeviceIoControl;
        let substitute: Vec<_> = OsString::from(r"\??\")
            .as_os_str()
            .encode_wide()
            .chain(target.as_os_str().encode_wide())
            .collect();
        let print: Vec<_> = target.as_os_str().encode_wide().collect();
        let mut data = Vec::new();
        data.extend_from_slice(&0xa000_0003u32.to_le_bytes()); // IO_REPARSE_TAG_MOUNT_POINT
        let data_len = 8 + (substitute.len() + print.len() + 2) * 2;
        data.extend_from_slice(&(data_len as u16).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes()); // reserved
        for field in [0, substitute.len() * 2, (substitute.len() + 1) * 2, print.len() * 2] {
            data.extend_from_slice(&(field as u16).to_le_bytes());
        }
        for c in substitute.into_iter().chain([0]).chain(print).chain([0]) {
            data.extend_from_slice(&c.to_le_bytes());
        }
        // DWORD storage aligns the REPARSE_DATA_BUFFER header for the API.
        let aligned: Vec<u32> = data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .chain((!data.len().is_multiple_of(4)).then(|| {
                let mut tail = [0; 4];
                let rest = &data[data.len() / 4 * 4..];
                tail[..rest.len()].copy_from_slice(rest);
                u32::from_le_bytes(tail)
            }))
            .collect();
        std::fs::create_dir(path).unwrap_or_else(|_| panic!("synthetic junction directory"));
        let handle = open(path, GENERIC_WRITE, &control()).unwrap_or_else(|_| panic!("synthetic junction handle"));
        let mut returned = 0;
        // SAFETY: handle opens our empty directory without following links;
        // the buffer encodes bounded mount-point offsets and UTF-16 names.
        // FSCTL_SET_REPARSE_POINT = 0x900a4; no output or async operation.
        assert_ne!(
            unsafe {
                DeviceIoControl(
                    handle.0,
                    0x900a4,
                    aligned.as_ptr().cast(),
                    data.len() as u32,
                    null_mut(),
                    0,
                    &mut returned,
                    null_mut(),
                )
            },
            0
        );
    }

    #[test]
    fn a_reparse_ancestor_is_refused_before_probing_beneath_it() {
        let parent = scratch();
        let target = parent.path().join("target");
        std::fs::create_dir(&target).unwrap_or_else(|_| panic!("synthetic junction target"));
        let link = parent.path().join("junction");
        junction(&link, &target);
        let below = link.join("does-not-exist");
        let control = control();
        // The first call must reject the junction attributes; a second
        // (child) probe would call the cancellation callback once more.
        let polls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = polls.clone();
        let counted = Control::new(std::sync::Arc::new(move || {
            observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            false
        }));
        assert_eq!(local(&link, &counted), Err(NATIVE));
        let at_ancestor = polls.swap(0, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(local(&below, &counted), Err(NATIVE));
        assert_eq!(polls.load(std::sync::atomic::Ordering::SeqCst), at_ancestor);
        assert_eq!(
            super::super::directory(&below, &control).err(),
            Some(ClosedError::UnsafeSource)
        );
        assert_eq!(
            super::super::create_private(&below, &control),
            Err(ClosedError::PrivateStorage)
        );
        assert_eq!(super::super::secure(&link, &control), Err(ClosedError::PrivateStorage));
        // Remove only the junction, leaving its target intact.
        std::fs::remove_dir(&link).unwrap_or_else(|_| panic!("synthetic junction removal"));
        assert!(target.is_dir());
    }

    #[test]
    fn native_actions_preserve_cancellation_and_deadlines() {
        let parent = scratch();
        let child = parent.path().join("never-created");
        for error in [ClosedError::Cancelled, ClosedError::TimedOut] {
            let mut stopped = Control::new(std::sync::Arc::new(move || error == ClosedError::Cancelled));
            stopped.deadline = std::time::Instant::now();
            assert_eq!(super::super::closed(&stopped), Err(error));
            assert_eq!(super::super::directory(parent.path(), &stopped).err(), Some(error));
            assert_eq!(super::super::space(parent.path(), 0, &stopped), Err(error));
            assert_eq!(super::super::secure(parent.path(), &stopped), Err(error));
            assert_eq!(super::super::create_private(&child, &stopped), Err(error));
            assert_eq!(backup(parent.path(), &stopped), Err(error));
            assert!(!child.exists());
        }
    }

    #[test]
    fn free_space_and_real_temp_backup_checks_use_native_apis() {
        let parent = scratch();
        assert_eq!(super::super::space(parent.path(), 0, &control()), Ok(()));
        assert_eq!(
            super::super::space(parent.path(), u64::MAX, &control()),
            Err(ClosedError::NoSpace)
        );
        let actual = real_temp(&control()).unwrap_or_else(|_| panic!("synthetic real temp"));
        let root = tempfile::tempdir_in(&actual).unwrap_or_else(|_| panic!("synthetic real temp directory"));
        assert_eq!(super::super::exclude_backups(root.path(), &control()), Ok(()));
        let attrs = attributes(
            &wide(root.path()).unwrap_or_else(|_| panic!("synthetic path")),
            &control(),
        )
        .unwrap_or_else(|_| panic!("synthetic attributes"));
        assert_ne!(attrs & FILE_ATTRIBUTE_NOT_CONTENT_INDEXED, 0);
        let outside = tempfile::Builder::new()
            .prefix("Temp-invented-")
            .tempdir_in(actual.parent().unwrap_or_else(|| panic!("synthetic temp parent")))
            .unwrap_or_else(|_| panic!("synthetic outside temp directory"));
        // Sharing the Temp prefix does not authorize a sibling directory.
        assert_eq!(
            super::super::exclude_backups(outside.path(), &control()),
            Err(ClosedError::PrivateStorage)
        );
    }
}
