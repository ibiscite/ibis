#![allow(clippy::disallowed_methods)]
//! Filesystem calls may block inside a driver, including metadata queries.
//! Bound the caller's wait and the number of detached calls; never join one.
use super::*;
use ibis_core::contract::Availability;
use std::sync::{Condvar, OnceLock};
use std::time::Instant;

const DEADLINE: Duration = Duration::from_secs(2);
const MAX_IN_FLIGHT: usize = 8;
const MAX_ATTACHMENT_PROBES: usize = 256;

#[derive(Clone, Default)]
struct Budget(Arc<(Mutex<usize>, Condvar)>);

struct Permit(Budget);
impl Drop for Permit {
    fn drop(&mut self) {
        let (count, available) = &*self.0.0;
        *count.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        available.notify_one();
    }
}

impl Budget {
    fn run<T: Send + 'static>(
        &self,
        scope: &Scope<'_>,
        operation: impl FnOnce() -> Result<T, SourceError> + Send + 'static,
    ) -> Result<T, SourceError> {
        let (count, available) = &*self.0;
        let mut count = count.lock().map_err(|_| SourceError::Unavailable)?;
        while *count >= MAX_IN_FLIGHT {
            let remaining = scope.remaining()?;
            count = available
                .wait_timeout(count, remaining.min(Duration::from_millis(10)))
                .map_err(|_| SourceError::Unavailable)?
                .0;
        }
        scope.remaining()?;
        *count += 1;
        drop(count);
        let permit = Permit(self.clone());
        let (send, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("ibis-file-io".into())
            .spawn(move || {
                let _permit = permit;
                let _ = send.send(operation());
            })
            .map_err(|_| SourceError::Unavailable)?;
        loop {
            let remaining = scope.remaining()?;
            match result.recv_timeout(remaining.min(Duration::from_millis(10))) {
                Ok(result) => return result,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(SourceError::Unavailable),
            }
        }
    }
}

/// One source load shares this absolute deadline, including capacity waits.
pub(super) struct Scope<'a> {
    until: Instant,
    cancelled: &'a dyn Fn() -> bool,
}

impl<'a> Scope<'a> {
    pub(super) fn new(cancelled: &'a dyn Fn() -> bool) -> Self {
        Self {
            until: Instant::now() + DEADLINE,
            cancelled,
        }
    }

    fn check_cancelled(&self) -> Result<(), SourceError> {
        if (self.cancelled)() {
            Err(SourceError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn remaining(&self) -> Result<Duration, SourceError> {
        self.check_cancelled()?;
        let remaining = self.until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            Err(SourceError::Unavailable)
        } else {
            Ok(remaining)
        }
    }

    pub(super) fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce() -> Result<T, SourceError> + Send + 'static,
    ) -> Result<T, SourceError> {
        static BUDGET: OnceLock<Budget> = OnceLock::new();
        BUDGET.get_or_init(Budget::default).run(self, operation)
    }

    pub(super) fn attachments(
        &self,
        objects: &mut [LibraryObject],
        base: &Path,
        references: Vec<(usize, String)>,
    ) -> Result<(), SourceError> {
        let base = base.to_path_buf();
        self.attachments_with(objects, references, move |location| {
            let path = base.join(location);
            if path.as_os_str().as_encoded_bytes().len() > crate::attachments::MAX_PATH_BYTES {
                Availability::Unknown
            } else {
                availability(&path)
            }
        })
    }

    fn attachments_with(
        &self,
        objects: &mut [LibraryObject],
        mut references: Vec<(usize, String)>,
        mut probe: impl FnMut(&str) -> Availability + Send + 'static,
    ) -> Result<(), SourceError> {
        if references.is_empty() {
            return self.check_cancelled();
        }
        references.truncate(MAX_ATTACHMENT_PROBES);
        references.shrink_to_fit();
        let until = self.until;
        let (send, results) = mpsc::channel();
        let outcome = self.run(move || {
            for (index, path) in references {
                if Instant::now() >= until {
                    break;
                }
                let availability = probe(&path);
                if Instant::now() >= until || send.send((index, availability)).is_err() {
                    break;
                }
            }
            Ok(())
        });
        // Only this caller owns the candidate. A late OS result has no way to
        // mutate it, and completed probes survive a later blocked mount.
        for (index, availability) in results.try_iter() {
            if let LibraryObject::Attachment(attachment) = &mut objects[index] {
                attachment.availability = availability;
            }
        }
        outcome.or_else(|error| match error {
            SourceError::Cancelled => Err(error),
            _ => self.check_cancelled(),
        })?;
        self.check_cancelled()
    }
}

pub(super) fn bounded<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, SourceError> + Send + 'static,
) -> Result<T, SourceError> {
    Scope::new(&|| false).run(operation)
}

fn availability(path: &Path) -> Availability {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return match error.kind() {
                std::io::ErrorKind::NotFound => Availability::BrokenLink,
                std::io::ErrorKind::PermissionDenied => Availability::Unreadable,
                _ => Availability::Unknown,
            };
        }
    };
    if !metadata.is_file() {
        return Availability::Unknown;
    }
    if needs_download(&metadata) {
        return Availability::CloudPlaceholder;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o444 == 0 {
            return Availability::Unreadable;
        }
    }
    Availability::OnDisk
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod probe_tests;

pub(super) fn read_library_file_sync(path: &Path) -> Result<String, SourceError> {
    let pause = || std::thread::sleep(Duration::from_millis(50));
    if path_needs_download(path)? {
        return Err(SourceError::Unavailable);
    }
    let metadata = retry_io(|| std::fs::metadata(path), pause).map_err(file_io_error)?;
    if !metadata.is_file() {
        return Err(SourceError::NotRegular);
    }
    if metadata.len() > MAX_LIBRARY_BYTES {
        return Err(SourceError::TooLarge);
    }
    read_if_downloaded(needs_download(&metadata), || {
        let file = retry_io(|| open_readonly(path), pause).map_err(file_io_error)?;
        read_open_file(file)
    })
}

// Keep the flag interpretation testable without a real cloud account. Windows
// OFFLINE and RECALL_ON_DATA_ACCESS; Darwin SF_DATALESS. RECALL_ON_OPEN is left
// out because its bit also means FILE_ATTRIBUTE_EA on ordinary local files.
#[cfg(any(windows, target_os = "macos", test))]
fn placeholder(windows_attributes: u32, darwin_flags: u32) -> bool {
    windows_attributes & (0x1000 | 0x400000) != 0 || darwin_flags & 0x40000000 != 0
}

pub(super) fn needs_download(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        placeholder(metadata.file_attributes(), 0)
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        placeholder(0, metadata.st_flags())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = metadata;
        false
    }
}

pub(super) fn path_needs_download(path: &Path) -> Result<bool, SourceError> {
    #[cfg(windows)]
    {
        use winsafe::co::FILE_ATTRIBUTE;
        let attributes =
            winsafe::GetFileAttributes(path.to_str().ok_or(SourceError::Unavailable)?).map_err(|error| {
                if error == winsafe::co::ERROR::FILE_NOT_FOUND || error == winsafe::co::ERROR::PATH_NOT_FOUND {
                    SourceError::Missing
                } else {
                    SourceError::Unavailable
                }
            })?;
        Ok(attributes.has(FILE_ATTRIBUTE::OFFLINE | FILE_ATTRIBUTE::RECALL_ON_DATA_ACCESS))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(legacy_icloud_placeholder(path) || needs_download(&std::fs::metadata(path).map_err(file_io_error)?))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = path;
        Ok(false)
    }
}

#[cfg(any(target_os = "macos", test))]
pub(super) fn legacy_icloud_placeholder(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.') && name.ends_with(".icloud"))
}

pub(super) fn read_if_downloaded<T>(
    download: bool,
    read: impl FnOnce() -> Result<T, SourceError>,
) -> Result<T, SourceError> {
    if download {
        Err(SourceError::Unavailable)
    } else {
        read()
    }
}

// Unknown and userspace filesystems are manual too: successful registration is
// not evidence that a remote client's writes will generate notifications.
#[cfg(any(target_os = "macos", test))]
fn local_darwin_filesystem(name: &str) -> bool {
    matches!(name, "apfs" | "hfs" | "msdos" | "exfat" | "udf" | "cd9660")
}

#[cfg(any(target_os = "linux", test))]
fn local_linux_filesystem(magic: u64) -> bool {
    matches!(
        magic,
        0xef53 | 0x58465342 | 0x9123683e | 0x01021994 | 0x858458f6 | 0x794c7630 | 0x2fc12fc1 | 0x4d44 | 0x2011bab0
    )
}

pub(super) fn supports_notifications(path: &Path) -> bool {
    let path = path.to_path_buf();
    bounded(move || {
        #[cfg(target_os = "macos")]
        let local = mount_at_or_above(&path).is_some_and(|mount| local_darwin_filesystem(mount.filesystem_type_name()));
        #[cfg(target_os = "linux")]
        let local =
            mount_at_or_above(&path).is_some_and(|mount| local_linux_filesystem(mount.filesystem_type().0 as u64));
        #[cfg(windows)]
        let local = path.to_str().is_some_and(|path| {
            winsafe::GetVolumePathName(path).is_ok_and(|volume| {
                matches!(
                    winsafe::GetDriveType(Some(&volume)),
                    winsafe::co::DRIVE::FIXED
                        | winsafe::co::DRIVE::REMOVABLE
                        | winsafe::co::DRIVE::RAMDISK
                        | winsafe::co::DRIVE::CDROM
                )
            })
        });
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        let local = {
            let _ = path;
            false
        };
        Ok(local)
    })
    .unwrap_or(false)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn mount_at_or_above(path: &Path) -> Option<nix::sys::statfs::Statfs> {
    for ancestor in path.ancestors() {
        match nix::sys::statfs::statfs(ancestor) {
            Ok(mount) => return Some(mount),
            Err(nix::errno::Errno::ENOENT | nix::errno::Errno::ENOTDIR) => continue,
            Err(_) => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_scope(duration: Duration) -> Scope<'static> {
        let mut scope = Scope::new(&|| false);
        scope.until = Instant::now() + duration;
        scope
    }

    #[test]
    fn network_and_unknown_filesystems_are_manual_even_without_events() {
        for name in ["nfs", "smbfs", "webdav", "osxfuse", "unknown"] {
            assert!(!local_darwin_filesystem(name));
        }
        for magic in [0x6969, 0xff534d42, 0xfe534d42, 0x65735546, 0] {
            assert!(!local_linux_filesystem(magic));
        }
        assert!(local_darwin_filesystem("apfs"));
        assert!(local_linux_filesystem(0xef53));
    }

    #[test]
    fn placeholders_are_deferred_before_reading_any_data() {
        // FILE_ATTRIBUTE_EA shares 0x40000 with RECALL_ON_OPEN, so an ordinary
        // local file with extended attributes must stay readable.
        assert!(!placeholder(0x40000, 0));
        for (attributes, flags) in [(0x1000, 0), (0x400000, 0), (0, 0x40000000)] {
            let result = read_if_downloaded(placeholder(attributes, flags), || -> Result<(), SourceError> {
                panic!("cloud data was opened before asking the person to download it")
            });
            assert_eq!(result, Err(SourceError::Unavailable));
        }
        // A locally hydrated reparse point or sparse file remains readable.
        assert_eq!(
            read_if_downloaded(placeholder(0x400 | 0x200, 0), || Ok("local references")),
            Ok("local references")
        );
        assert_eq!(
            read_if_downloaded(false, || Err::<(), _>(SourceError::Unreadable)),
            Err(SourceError::Unreadable)
        );
        assert!(legacy_icloud_placeholder(Path::new(".synthetic.bib.icloud")));
        assert!(!legacy_icloud_placeholder(Path::new("synthetic.bib")));
    }

    #[test]
    fn stuck_calls_have_a_fixed_resource_limit() {
        let budget = Budget::default();
        let mut releases = Vec::new();
        for _ in 0..MAX_IN_FLIGHT {
            let (release, stuck) = mpsc::channel::<()>();
            releases.push(release);
            assert_eq!(
                budget.run(&test_scope(Duration::from_millis(10)), move || {
                    stuck.recv().map_err(|_| SourceError::Unavailable)
                }),
                Err(SourceError::Unavailable)
            );
        }
        assert_eq!(
            budget.run(&test_scope(Duration::from_secs(1)), || -> Result<(), SourceError> {
                panic!("the detached I/O limit was exceeded")
            }),
            Err(SourceError::Unavailable)
        );
        drop(releases);
    }
}
