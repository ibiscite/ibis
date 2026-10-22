//! Owned native dialogs. No platform calls or payloads; dropping a lease
//! releases only its owner, including when a callback is discarded or unwinds.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogKind {
    Chooser,
    Message,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    ChooserOpen,
    Unavailable,
}

#[derive(Default)]
struct Registry {
    next: u64,
    owners: BTreeMap<u64, DialogKind>,
}

#[derive(Clone, Default)]
pub(crate) struct DialogOpen(Arc<Mutex<Registry>>);

#[must_use]
pub(crate) struct DialogLease {
    dialogs: DialogOpen,
    owner: u64,
}

impl DialogOpen {
    pub(crate) fn begin(&self, kind: DialogKind) -> Result<DialogLease, Refused> {
        let mut registry = self.0.lock().map_err(|_| Refused::Unavailable)?;
        if kind == DialogKind::Chooser && registry.owners.values().any(|kind| *kind == DialogKind::Chooser) {
            return Err(Refused::ChooserOpen);
        }
        let owner = registry.next.checked_add(1).ok_or(Refused::Unavailable)?;
        registry.next = owner;
        registry.owners.insert(owner, kind);
        Ok(DialogLease {
            dialogs: self.clone(),
            owner,
        })
    }

    pub(crate) fn is_open(&self) -> bool {
        self.0
            .lock()
            .map(|r| r.next == u64::MAX || !r.owners.is_empty())
            .unwrap_or(true)
    }

    fn release(&self, owner: u64) {
        if let Ok(mut registry) = self.0.lock() {
            registry.owners.remove(&owner);
        }
    }
}

impl DialogLease {
    /// Hold ownership through the whole callback, including a source change
    /// or the acquisition of a nested message's lease, without holding a lock.
    pub(crate) fn during<T>(self, action: impl FnOnce() -> T) -> T {
        let _lease = self;
        action()
    }
}

impl Drop for DialogLease {
    fn drop(&mut self) {
        self.dialogs.release(self.owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(dialogs: &DialogOpen) -> DialogLease {
        dialogs.begin(DialogKind::Message).unwrap_or_else(|e| panic!("{e:?}"))
    }

    #[test]
    fn completing_one_dialog_keeps_another_protected() {
        let dialogs = DialogOpen::default();
        let first = message(&dialogs);
        let second = message(&dialogs);
        drop(first);
        assert!(dialogs.is_open());
        drop(second);
        assert!(!dialogs.is_open());
    }

    #[test]
    fn stale_and_duplicate_completion_cannot_release_another_owner() {
        let dialogs = DialogOpen::default();
        let first = message(&dialogs);
        let id = first.owner;
        drop(first);
        let second = message(&dialogs);
        dialogs.release(id);
        dialogs.release(id);
        assert!(dialogs.is_open());
        drop(second);
        assert!(!dialogs.is_open());
    }

    #[test]
    fn chooser_is_exclusive_while_message_owners_remain_independent() {
        let dialogs = DialogOpen::default();
        let message = message(&dialogs);
        let chooser = dialogs.begin(DialogKind::Chooser).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(dialogs.begin(DialogKind::Chooser).err(), Some(Refused::ChooserOpen));
        drop(message);
        assert_eq!(dialogs.begin(DialogKind::Chooser).err(), Some(Refused::ChooserOpen));
        drop(chooser);
        assert!(dialogs.begin(DialogKind::Chooser).is_ok());
    }

    #[test]
    fn callback_holds_ownership_through_source_work_and_nested_dialogs() {
        let dialogs = DialogOpen::default();
        let nested = message(&dialogs).during(|| {
            assert!(dialogs.is_open());
            assert!(dialogs.0.try_lock().is_ok());
            message(&dialogs)
        });
        assert!(dialogs.is_open());
        drop(nested);
        assert!(!dialogs.is_open());
    }

    #[test]
    fn discarded_and_panicking_callbacks_release_their_own_lease() {
        let dialogs = DialogOpen::default();
        let lease = message(&dialogs);
        let callback = move || lease.during(|| {});
        drop(callback);
        assert!(!dialogs.is_open());
        let other = message(&dialogs);
        let lease = message(&dialogs);
        assert!(std::panic::catch_unwind(|| lease.during(|| panic!("synthetic callback"))).is_err());
        assert!(dialogs.is_open());
        drop(other);
        assert!(!dialogs.is_open());
    }

    #[test]
    fn poison_and_exhausted_owner_ids_refuse_new_dialogs_and_fail_closed() {
        let dialogs = DialogOpen::default();
        dialogs.0.lock().unwrap_or_else(|e| panic!("{e}")).next = u64::MAX;
        assert_eq!(dialogs.begin(DialogKind::Message).err(), Some(Refused::Unavailable));
        assert!(dialogs.is_open());
        let dialogs = DialogOpen::default();
        assert!(
            std::panic::catch_unwind(|| {
                let _locked = dialogs.0.lock().unwrap_or_else(|e| panic!("{e}"));
                panic!("synthetic poison");
            })
            .is_err()
        );
        assert_eq!(dialogs.begin(DialogKind::Message).err(), Some(Refused::Unavailable));
        assert!(dialogs.is_open());
    }
}
