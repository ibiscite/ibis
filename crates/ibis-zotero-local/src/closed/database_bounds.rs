//! Bound SQLite's logical size before its pager/quick_check can allocate from
//! untrusted main or WAL page-count claims. Checksums and recovery stay SQLite's
//! responsibility; we only read the documented fixed headers of our private copy.
use super::private_copy::{Copy, MAX_BYTES};
use super::{ClosedError, Control};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub(super) fn check(copy: &Copy, control: &Control) -> Result<(), ClosedError> {
    let path = copy.database_path();
    let mut main = File::open(&path).map_err(|_| ClosedError::PrivateStorage)?;
    let mut header = [0; 100];
    main.read_exact(&mut header).map_err(|_| ClosedError::Damaged)?;
    if &header[..16] != b"SQLite format 3\0" {
        return Err(ClosedError::Damaged);
    }
    let encoded = u16::from_be_bytes([header[16], header[17]]) as u64;
    let size = if encoded == 1 { 65536 } else { encoded };
    if !(512..=65536).contains(&size) || !size.is_power_of_two() {
        return Err(ClosedError::Damaged);
    }
    let max = MAX_BYTES / size;
    bound(&header[28..32], max)?;
    let mut wal_path = path.into_os_string();
    wal_path.push("-wal");
    let mut wal = match File::open(wal_path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(ClosedError::PrivateStorage),
    };
    let length = wal.metadata().map_err(|_| ClosedError::PrivateStorage)?.len();
    if length == 0 {
        return Ok(());
    }
    let mut wal_header = [0; 32];
    wal.read_exact(&mut wal_header).map_err(|_| ClosedError::Damaged)?;
    if ![0x377f0682, 0x377f0683].contains(&u32_at(&wal_header[..4])) || u64::from(u32_at(&wal_header[8..12])) != size {
        return Err(ClosedError::Damaged);
    }
    let frame = size + 24;
    let frames = (length - 32) / frame;
    for i in 0..frames {
        control.check()?;
        wal.seek(SeekFrom::Start(32 + i * frame))
            .map_err(|_| ClosedError::Damaged)?;
        let mut frame_header = [0; 24];
        wal.read_exact(&mut frame_header).map_err(|_| ClosedError::Damaged)?;
        bound(&frame_header[..4], max)?;
        bound(&frame_header[4..8], max)?;
        if u32_at(&frame_header[..4]) == 1 {
            let mut page_header = [0; 100];
            wal.read_exact(&mut page_header).map_err(|_| ClosedError::Damaged)?;
            bound(&page_header[28..32], max)?;
        }
    }
    Ok(())
}
fn u32_at(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn bound(bytes: &[u8], max: u64) -> Result<(), ClosedError> {
    if u64::from(u32_at(bytes)) > max {
        Err(ClosedError::TooLarge)
    } else {
        Ok(())
    }
}
