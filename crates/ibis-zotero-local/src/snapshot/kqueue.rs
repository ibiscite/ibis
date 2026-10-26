//! Retained vnode witnesses. Reading evidence never makes a dirty interval clean.
use std::fs::File;
use std::os::fd::AsRawFd;

use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::sys::event::{EvFlags, EventFilter, FilterFlag, KEvent, Kqueue};

use super::SnapshotError;

pub(super) struct Witness {
    queue: Kqueue,
    handles: Vec<File>,
    failed: bool,
}

impl Witness {
    pub(super) fn new() -> Result<Self, SnapshotError> {
        let queue = Kqueue::new().map_err(|_| SnapshotError::Changing)?;
        fcntl(&queue, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).map_err(|_| SnapshotError::Changing)?;
        Ok(Self {
            queue,
            handles: Vec::new(),
            failed: false,
        })
    }

    pub(super) fn watch(&mut self, file: &File) -> Result<(), SnapshotError> {
        let retained = file.try_clone().map_err(|_| SnapshotError::Changing)?;
        let change = KEvent::new(
            retained.as_raw_fd() as usize,
            EventFilter::EVFILT_VNODE,
            EvFlags::EV_ADD | EvFlags::EV_ENABLE | EvFlags::EV_CLEAR | EvFlags::EV_RECEIPT,
            FilterFlag::NOTE_WRITE
                | FilterFlag::NOTE_EXTEND
                | FilterFlag::NOTE_ATTRIB
                | FilterFlag::NOTE_LINK
                | FilterFlag::NOTE_RENAME
                | FilterFlag::NOTE_DELETE
                | FilterFlag::NOTE_REVOKE,
            0,
            0,
        );
        let mut receipt = [change];
        let result = self.queue.kevent(
            &[change],
            &mut receipt,
            Some(nix::libc::timespec { tv_sec: 0, tv_nsec: 0 }),
        );
        if !matches!(result, Ok(1))
            || receipt[0].ident() != change.ident()
            || !receipt[0].flags().contains(EvFlags::EV_ERROR)
            || receipt[0].data() != 0
        {
            self.failed = true;
            return Err(SnapshotError::Changing);
        }
        self.handles.push(retained);
        self.unchanged()
    }

    pub(super) fn unchanged(&mut self) -> Result<(), SnapshotError> {
        if self.failed {
            return Err(SnapshotError::Changing);
        }
        let blank = KEvent::new(
            0,
            EventFilter::EVFILT_VNODE,
            EvFlags::empty(),
            FilterFlag::empty(),
            0,
            0,
        );
        let mut events = [blank; 1];
        match self
            .queue
            .kevent(&[], &mut events, Some(nix::libc::timespec { tv_sec: 0, tv_nsec: 0 }))
        {
            Ok(0) => Ok(()),
            _ => {
                self.failed = true;
                Err(SnapshotError::Changing)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_inode_reports_writes_after_the_callers_handle_closes() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("source");
        std::fs::write(&path, b"before").unwrap_or_else(|e| panic!("{e}"));
        let mut witness = Witness::new().unwrap_or_else(|e| panic!("{e}"));
        let file = File::open(&path).unwrap_or_else(|e| panic!("{e}"));
        witness.watch(&file).unwrap_or_else(|e| panic!("{e}"));
        drop(file);
        std::fs::write(path, b"after!").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(witness.unchanged(), Err(SnapshotError::Changing));
        assert_eq!(witness.unchanged(), Err(SnapshotError::Changing));
    }
    #[test]
    fn enrolling_another_inode_does_not_erase_prior_mutation() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("source");
        std::fs::write(&path, b"before").unwrap_or_else(|e| panic!("{e}"));
        let first = File::open(&path).unwrap_or_else(|e| panic!("{e}"));
        let second = File::open(dir.path()).unwrap_or_else(|e| panic!("{e}"));
        let mut witness = Witness::new().unwrap_or_else(|e| panic!("{e}"));
        witness.watch(&first).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(path, b"after!").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(witness.watch(&second), Err(SnapshotError::Changing));
    }
}
