//! Win32 source-locality, process and free-space checks. Cache files use core's creator.
use super::{ClosedError, Control, drive_path};
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetFileAttributesW, INVALID_FILE_ATTRIBUTES,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
const NONLOCAL: u32 = 0x0044_1400;
const PATH_CAPACITY: usize = 32_768;

struct Handle(HANDLE);
impl Handle {
    fn new(raw: HANDLE) -> Result<Self, ClosedError> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(ClosedError::PrivateStorage)
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

fn wide(path: &Path) -> Result<Vec<u16>, ClosedError> {
    let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
    if value.is_empty() || value.len() >= PATH_CAPACITY || value.contains(&0) {
        return Err(ClosedError::UnsafeSource);
    }
    value.push(0);
    Ok(value)
}

// Check the raw spelling too: Path::components silently removes interior '.'.
fn checked_path(path: &Path) -> Result<Vec<u16>, ClosedError> {
    let value = wide(path)?;
    if !drive_path(path) {
        return Err(ClosedError::UnsafeSource);
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
            return Err(ClosedError::UnsafeSource);
        }
    }
    Ok(value)
}

fn attributes(path: &[u16], control: &Control) -> Result<u32, ClosedError> {
    control.check()?;
    // SAFETY: all callers provide a live, NUL-terminated UTF-16 path.
    let attributes = unsafe { GetFileAttributesW(path.as_ptr()) };
    if attributes == INVALID_FILE_ATTRIBUTES || attributes & NONLOCAL != 0 {
        return Err(ClosedError::UnsafeSource);
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
        return Err(ClosedError::UnsafeSource);
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
    let snapshot = Handle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })
        .map_err(|_| ClosedError::ProcessCheck)?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    control.check()?;
    // SAFETY: entry is initialized and advertises the exact structure size.
    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return Err(ClosedError::ProcessCheck);
    }
    let mut seen_self = false;
    let mut running = false;
    loop {
        control.check()?;
        seen_self |= entry.th32ProcessID == std::process::id();
        let len = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .ok_or(ClosedError::ProcessCheck)?;
        running |= zotero_image(&entry.szExeFile[..len]);
        control.check()?;
        // SAFETY: snapshot is live and entry still has its required size.
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            // SAFETY: read immediately after the failed enumeration call.
            if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
                return Err(ClosedError::ProcessCheck);
            }
            break;
        }
    }
    control.check()?;
    if !seen_self {
        Err(ClosedError::ProcessCheck)
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
    if unsafe { GetDiskFreeSpaceExW(value.as_ptr(), &mut available, null_mut(), null_mut()) } == 0 {
        return Err(ClosedError::NoSpace);
    }
    control.check()?;
    if available < needed {
        Err(ClosedError::NoSpace)
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn open(path: &Path, access: u32, control: &Control) -> Result<Handle, ClosedError> {
    use std::ptr::null;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::closed::tests::control;
    use std::ffi::OsString;
    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
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
        assert_eq!(local(&link, &counted), Err(ClosedError::UnsafeSource));
        let at_ancestor = polls.swap(0, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(local(&below, &counted), Err(ClosedError::UnsafeSource));
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
    fn text_cache_refuses_a_junction_to_another_attachment() {
        let data = scratch();
        let storage = data.path().join("storage");
        let secret = storage.join("SECRET01");
        std::fs::create_dir_all(&secret).unwrap();
        std::fs::write(secret.join(".zotero-ft-cache"), "Invented secret text").unwrap();
        let link = storage.join("ALLOW001");
        junction(&link, &secret);
        assert!(crate::closed::read_text_cache(data.path(), "ALLOW001", std::sync::Arc::new(|| false)).is_err());
        std::fs::remove_dir(&link).unwrap();
    }
}
