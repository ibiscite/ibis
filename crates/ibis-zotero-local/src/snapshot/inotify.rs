//! Retained mutation witnesses for local Linux files and directories.
//!
//! Source handles remain open in `Sources`. Following their procfs links
//! enrolls the retained inodes, including changes through other hard links.
//! This never locks a source or asks another process to wait.

use std::fs::File;
use std::mem::MaybeUninit;
use std::os::fd::AsRawFd;

use rustix::fd::OwnedFd;
use rustix::fs::inotify::{self, CreateFlags, WatchFlags};

use super::SnapshotError;

const MUTATIONS: WatchFlags = WatchFlags::MODIFY
    .union(WatchFlags::ATTRIB)
    .union(WatchFlags::CLOSE_WRITE)
    .union(WatchFlags::CREATE)
    .union(WatchFlags::DELETE)
    .union(WatchFlags::DELETE_SELF)
    .union(WatchFlags::MOVE_SELF)
    .union(WatchFlags::MOVED_FROM)
    .union(WatchFlags::MOVED_TO);

pub(super) struct Witness {
    events: OwnedFd,
    armed: bool,
    failed: bool,
}

impl Witness {
    pub(super) fn new() -> Result<Self, SnapshotError> {
        Ok(Self {
            events: inotify::init(CreateFlags::NONBLOCK | CreateFlags::CLOEXEC).map_err(|_| SnapshotError::Changing)?,
            armed: false,
            failed: false,
        })
    }

    pub(super) fn watch(&mut self, file: &File) -> Result<(), SnapshotError> {
        if self.failed {
            return Err(SnapshotError::Changing);
        }
        // Do not use DONT_FOLLOW: the procfs link must resolve to the held
        // inode, rather than to the link itself. Re-enrollment of a hard link
        // uses the same complete mask and does not read or clear events.
        let path = format!("/proc/self/fd/{}", file.as_raw_fd());
        if inotify::add_watch(&self.events, path, MUTATIONS).is_err() {
            self.failed = true;
            return Err(SnapshotError::Changing);
        }
        self.armed = true;
        Ok(())
    }

    pub(super) fn unchanged(&mut self) -> Result<(), SnapshotError> {
        if self.failed || !self.armed {
            return Err(SnapshotError::Changing);
        }
        // Any event invalidates the interval, including IGNORED, UNMOUNT,
        // queue overflow and future event types. A single read is sufficient:
        // only EAGAIN proves that no event is pending. Retain failure even
        // after consuming the event so later checks cannot accept the copy.
        let mut buffer = [MaybeUninit::uninit(); 512];
        if matches!(
            inotify::Reader::new(&self.events, &mut buffer).next(),
            Err(rustix::io::Errno::AGAIN)
        ) {
            return Ok(());
        }
        self.failed = true;
        Err(SnapshotError::Changing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watched() -> (tempfile::TempDir, File, Witness) {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(dir.path().join("source"), b"synthetic before").unwrap_or_else(|e| panic!("{e}"));
        let file = File::open(dir.path().join("source")).unwrap_or_else(|e| panic!("{e}"));
        let mut witness = Witness::new().unwrap_or_else(|e| panic!("{e}"));
        witness.watch(&file).unwrap_or_else(|e| panic!("{e}"));
        (dir, file, witness)
    }

    #[test]
    fn reads_do_not_invalidate_or_block_writers() {
        let (dir, file, mut witness) = watched();
        assert_eq!(
            std::fs::read(dir.path().join("source")).ok().as_deref(),
            Some(b"synthetic before".as_slice())
        );
        let flags = rustix::fs::fcntl_getfl(&witness.events).unwrap_or_else(|e| panic!("{e}"));
        assert!(flags.contains(rustix::fs::OFlags::NONBLOCK));
        assert_eq!(witness.unchanged(), Ok(()));
        // A second enrollment must preserve earlier events.
        std::fs::write(dir.path().join("source"), b"synthetic after!").unwrap_or_else(|e| panic!("{e}"));
        witness.watch(&file).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(witness.unchanged(), Err(SnapshotError::Changing));
        assert_eq!(witness.unchanged(), Err(SnapshotError::Changing));
    }

    #[test]
    fn retained_inode_observes_writes_through_an_external_hard_link() {
        let (dir, file, _) = watched();
        let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        std::fs::hard_link(dir.path().join("source"), outside.path().join("alias")).unwrap_or_else(|e| panic!("{e}"));
        // Use a fresh witness after link creation, so the write itself is
        // tested rather than the preceding link-count metadata event.
        let mut witness = Witness::new().unwrap_or_else(|e| panic!("{e}"));
        witness.watch(&file).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(outside.path().join("alias"), b"synthetic after!").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(witness.unchanged(), Err(SnapshotError::Changing));
    }

    #[test]
    fn directory_witness_remembers_a_complete_sidecar_lifecycle() {
        let (dir, _file, _) = watched();
        let parent = File::open(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let mut witness = Witness::new().unwrap_or_else(|e| panic!("{e}"));
        witness.watch(&parent).unwrap_or_else(|e| panic!("{e}"));
        let sidecar = dir.path().join("source-wal");
        std::fs::write(&sidecar, b"synthetic wal").unwrap_or_else(|e| panic!("{e}"));
        std::fs::remove_file(sidecar).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(witness.unchanged(), Err(SnapshotError::Changing));
    }

    #[test]
    fn rename_away_and_back_does_not_restore_the_witness() {
        let (dir, _file, mut witness) = watched();
        let source = dir.path().join("source");
        let moved = dir.path().join("moved");
        std::fs::rename(&source, &moved).unwrap_or_else(|e| panic!("{e}"));
        std::fs::rename(&moved, &source).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(witness.unchanged(), Err(SnapshotError::Changing));
    }

    #[test]
    fn lost_watches_fail_closed() {
        let (_dir, file, mut witness) = watched();
        let path = format!("/proc/self/fd/{}", file.as_raw_fd());
        let id = inotify::add_watch(&witness.events, path, MUTATIONS).unwrap_or_else(|e| panic!("{e}"));
        inotify::remove_watch(&witness.events, id).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(witness.unchanged(), Err(SnapshotError::Changing));
    }
}
