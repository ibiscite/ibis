//! The kernel's mount list, read with `getfsstat(MNT_NOWAIT)`: the cached
//! table `df -n` prints, without starting a process and without asking any
//! file system, so a network mount is never contacted.

use nix::libc;
use std::ffi::{CStr, OsString};
use std::os::unix::ffi::OsStringExt;

/// Each mount's point and file system type, or `None` when the list is
/// longer than `limit` or a row cannot be read.
pub(super) fn mounts(limit: usize) -> Option<Vec<(OsString, String)>> {
    let mut capacity = 64usize.min(limit.saturating_add(1));
    loop {
        // SAFETY: statfs is plain old data; all zeroes is a valid value.
        let empty: libc::statfs = unsafe { std::mem::zeroed() };
        let mut table = vec![empty; capacity];
        let bytes = libc::c_int::try_from(capacity.checked_mul(std::mem::size_of::<libc::statfs>())?).ok()?;
        // SAFETY: the buffer holds `capacity` initialized records and `bytes`
        // is its exact size; the kernel writes whole records within it and
        // returns how many, or -1.
        let count = unsafe { libc::getfsstat(table.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
        let count = usize::try_from(count).ok()?;
        // A full buffer may have cut the list: read again, larger.
        if count < capacity {
            table.truncate(count);
            return table.iter().map(row).collect();
        }
        if capacity > limit {
            return None;
        }
        capacity = capacity.saturating_mul(4).min(limit.saturating_add(1));
    }
}

fn row(mount: &libc::statfs) -> Option<(OsString, String)> {
    let point = text(&mount.f_mntonname)?.to_bytes().to_vec();
    let kind = text(&mount.f_fstypename)?.to_str().ok()?.to_owned();
    Some((OsString::from_vec(point), kind))
}

/// A fixed field's text up to its terminating NUL; none without one.
fn text(field: &[libc::c_char]) -> Option<&CStr> {
    // SAFETY: c_char and u8 have the same size and alignment, and the slice
    // borrows the record for its own lifetime.
    let bytes = unsafe { std::slice::from_raw_parts(field.as_ptr().cast::<u8>(), field.len()) };
    CStr::from_bytes_until_nul(bytes).ok()
}
