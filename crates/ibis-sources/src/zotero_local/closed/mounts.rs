//! Classify the cached mount table before probing a path. `statfs(path)` can
//! itself perform a network RPC, so it cannot be the locality gate.

use std::collections::VecDeque;
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use super::{ClosedError, Control};

// SAFETY: one libc call that fills a buffer this module owns.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos;

#[cfg(any(target_os = "linux", test))]
const MAX_TABLE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MOUNTS: usize = 32_768;

struct Mount {
    point: PathBuf,
    local: bool,
}

// Unknown and layered/user-space filesystems fail closed. In particular,
// FUSE, autofs, overlay and clustered/network filesystems are unsupported.
fn local_kind(kind: &str) -> bool {
    matches!(
        kind.trim(),
        "apfs"
            | "hfs"
            | "ext2"
            | "ext3"
            | "ext4"
            | "xfs"
            | "btrfs"
            | "f2fs"
            | "tmpfs"
            | "ramfs"
            | "zfs"
            | "bcachefs"
            | "jfs"
            | "reiserfs"
            | "nilfs2"
            | "squashfs"
            | "erofs"
            | "vfat"
            | "msdos"
            | "exfat"
            | "ntfs3"
            | "ntfs"
            | "ubifs"
            | "jffs2"
            | "cramfs"
            | "romfs"
            | "iso9660"
            | "cd9660"
            | "udf"
    )
}

/// Any non-ASCII mismatch is ambiguous across case/normalization rules.
/// This deliberately over-approximates aliases, without inventing APFS or
/// per-directory Linux casefold semantics. ASCII-disjoint components prove
/// disjointness, so a remote /Volumes mount does not reject /Users/研究.
fn possible_prefix(mount: &Path, path: &Path) -> bool {
    let mut path = path.components();
    for component in mount.components() {
        let Some(candidate) = path.next() else { return false };
        let a = component.as_os_str().as_encoded_bytes();
        let b = candidate.as_os_str().as_encoded_bytes();
        if a.is_ascii() && b.is_ascii() && !a.eq_ignore_ascii_case(b) {
            return false;
        }
    }
    true
}

fn eligible(path: &Path, mounts: &[Mount]) -> bool {
    path.is_absolute()
        && mounts.iter().any(|mount| mount.local && path.starts_with(&mount.point))
        && !mounts
            .iter()
            .any(|mount| !mount.local && possible_prefix(&mount.point, path))
}

fn complete(mounts: Vec<Mount>) -> Option<Vec<Mount>> {
    (!mounts.is_empty() && mounts.len() <= MAX_MOUNTS && mounts.iter().any(|mount| mount.point == Path::new("/")))
        .then_some(mounts)
}

#[cfg(any(target_os = "linux", test))]
fn linux_table(bytes: &[u8]) -> Option<Vec<Mount>> {
    use std::os::unix::ffi::OsStringExt;
    if bytes.len() as u64 > MAX_TABLE_BYTES {
        return None;
    }
    let mut mounts = Vec::new();
    for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
        if mounts.len() >= MAX_MOUNTS {
            return None;
        }
        let fields: Vec<_> = line.split(|b| *b == b' ').collect();
        let separator = fields.iter().position(|field| *field == b"-")?;
        if separator < 6 {
            return None;
        }
        let point = decode_mount_path(fields.get(4)?)?;
        let point = PathBuf::from(OsString::from_vec(point));
        if !point.is_absolute() {
            return None;
        }
        let kind = std::str::from_utf8(fields.get(separator + 1)?).ok()?;
        mounts.push(Mount {
            point,
            local: local_kind(kind),
        });
    }
    complete(mounts)
}

#[cfg(any(target_os = "linux", test))]
fn decode_mount_path(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut rest = bytes;
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'\\' {
            let escape = rest.get(..4)?;
            result.push(match escape {
                b"\\040" => b' ',
                b"\\011" => b'\t',
                b"\\012" => b'\n',
                b"\\134" => b'\\',
                _ => return None,
            });
            rest = &rest[4..];
        } else {
            result.push(byte);
            rest = tail;
        }
    }
    Some(result)
}

/// A macOS mount list as [`macos::mounts`] gives it.
#[cfg(any(target_os = "macos", test))]
fn macos_table(rows: Vec<(OsString, String)>) -> Option<Vec<Mount>> {
    let mounts = rows
        .into_iter()
        .map(|(point, kind)| {
            let point = PathBuf::from(point);
            point.is_absolute().then(|| Mount {
                point,
                local: local_kind(&kind),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    complete(mounts)
}

#[cfg(target_os = "linux")]
fn cached(control: &Control) -> Result<Vec<Mount>, ClosedError> {
    control.check()?;
    let mut bytes = Vec::new();
    let read = std::fs::File::open("/proc/self/mountinfo")
        .and_then(|file| file.take(MAX_TABLE_BYTES + 1).read_to_end(&mut bytes));
    control.check()?;
    read.map_err(|_| ClosedError::UnsafeSource)?;
    let mounts = linux_table(&bytes).ok_or(ClosedError::UnsafeSource);
    control.check()?;
    mounts
}

#[cfg(target_os = "macos")]
fn cached(control: &Control) -> Result<Vec<Mount>, ClosedError> {
    control.check()?;
    // One system call, not `df`: a read never depends on starting a
    // process, which fails when the machine is out of processes.
    let mounts = macos::mounts(MAX_MOUNTS)
        .and_then(macos_table)
        .ok_or(ClosedError::UnsafeSource);
    control.check()?;
    mounts
}

pub(super) fn resolved(path: &Path, control: &Control) -> Result<PathBuf, ClosedError> {
    control.check()?;
    let mounts = cached(control)?;
    let resolved = walk(path, &mounts, |path| std::fs::symlink_metadata(path));
    control.check()?;
    resolved
}

/// Classify a path before opening it, without probing any component. The
/// caller opens each component through no-follow handles.
pub(super) fn local(path: &Path, control: &Control) -> Result<(), ClosedError> {
    control.check()?;
    let mounts = cached(control)?;
    if !path.is_absolute() {
        return Err(ClosedError::UnsafeSource);
    }
    let mut current = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::Normal(name) => current.push(name),
            Component::ParentDir => {
                current.pop();
            }
            Component::RootDir | Component::CurDir => (),
            Component::Prefix(_) => return Err(ClosedError::UnsafeSource),
        }
        if !eligible(&current, &mounts) {
            return Err(ClosedError::UnsafeSource);
        }
    }
    control.check()
}

fn walk(
    path: &Path,
    mounts: &[Mount],
    metadata: impl Fn(&Path) -> std::io::Result<std::fs::Metadata>,
) -> Result<PathBuf, ClosedError> {
    if !path.is_absolute() {
        return Err(ClosedError::UnsafeSource);
    }
    let mut remaining: VecDeque<OsString> = path
        .components()
        .skip(1)
        .map(|part| part.as_os_str().to_os_string())
        .collect();
    let mut current = PathBuf::from("/");
    let mut links = 0;
    if !eligible(&current, mounts) {
        return Err(ClosedError::UnsafeSource);
    }
    while let Some(part) = remaining.pop_front() {
        if part == "." {
            continue;
        }
        if part == ".." {
            current.pop();
            continue;
        }
        current.push(part);
        if !eligible(&current, mounts) {
            return Err(ClosedError::UnsafeSource);
        }
        let seen = metadata(&current).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ClosedError::NotFound
            } else {
                ClosedError::UnsafeSource
            }
        })?;
        if !super::resident(&seen) {
            return Err(ClosedError::UnsafeSource);
        }
        if seen.file_type().is_symlink() {
            links += 1;
            if links > 40 {
                return Err(ClosedError::UnsafeSource);
            }
            let target = std::fs::read_link(&current).map_err(|_| ClosedError::UnsafeSource)?;
            current.pop();
            if target.is_absolute() {
                current = PathBuf::from("/");
            }
            let parts = target.components().filter(|part| !matches!(part, Component::RootDir));
            for part in parts.collect::<Vec<_>>().into_iter().rev() {
                remaining.push_front(part.as_os_str().to_os_string());
            }
        } else if !seen.is_dir() {
            return Err(ClosedError::UnsafeSource);
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn unknown_remote_and_fuse_mounts_are_rejected_without_a_probe() {
        for kind in [
            "nfs",
            "nfs4",
            "cifs",
            "fuse.sshfs",
            "fuse.rclone",
            "afs",
            "ncpfs",
            "ocfs2",
            "autofs",
            "overlay",
            "futurefs",
        ] {
            let table =
                format!("1 0 8:1 / / rw - ext4 /dev/synthetic rw\n2 1 0:2 / /mnt/remote rw - {kind} synthetic rw\n");
            let mounts = linux_table(table.as_bytes()).unwrap_or_else(|| panic!("synthetic table"));
            let probes = Cell::new(0);
            let checked = walk(Path::new("/mnt/remote/Zotero"), &mounts, |path| {
                probes.set(probes.get() + 1);
                if path == Path::new("/mnt") {
                    std::fs::metadata("/")
                } else {
                    panic!("remote path was probed")
                }
            });
            assert_eq!(checked, Err(ClosedError::UnsafeSource), "{kind}");
            assert_eq!(probes.get(), 1);
        }
    }

    #[test]
    fn macos_mounts_refuse_case_and_unicode_aliases_conservatively() {
        let rows = [("/", "apfs"), ("/Volumes/Remote", "smbfs"), ("/Volumes/\u{3c2}", "nfs")]
            .map(|(point, kind)| (OsString::from(point), kind.to_owned()));
        let mounts = macos_table(rows.to_vec()).unwrap_or_else(|| panic!("table"));
        assert!(!eligible(Path::new("/volumes/remote/Zotero"), &mounts));
        assert!(!eligible(Path::new("/Volumes/σ/Zotero"), &mounts));
        assert!(eligible(Path::new("/Users/研究/Zotero"), &mounts));
        assert!(eligible(Path::new("/Users/test/Documents/Zotero"), &mounts));
    }

    #[test]
    fn a_local_bind_below_a_remote_ancestor_does_not_authorize_traversal() {
        let table = b"1 0 8:1 / / rw - ext4 /dev/synthetic rw\n2 1 0:2 / /remote rw - nfs synthetic rw\n3 2 8:1 / /remote/local-bind rw - ext4 /dev/synthetic rw\n";
        let mounts = linux_table(table).unwrap_or_else(|| panic!("table"));
        assert_eq!(
            walk(Path::new("/remote/local-bind/Zotero"), &mounts, |_| panic!(
                "the remote ancestor must not be probed"
            )),
            Err(ClosedError::UnsafeSource)
        );
        assert!(linux_table(b"2 1 8:1 / /only-child rw - ext4 synthetic rw\n").is_none());
        assert!(linux_table(&vec![b'x'; MAX_TABLE_BYTES as usize + 1]).is_none());
    }
}
