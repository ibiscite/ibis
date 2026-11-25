//! One owner for state that newer work replaces.
//!
//! An [`Epoch`] holds the state itself, so a late completion cannot write it
//! by any route but [`Stamp::commit`]. Beginning work, retiring it and the
//! whole guarded update run under one lock: once a newer [`Epoch::begin`]
//! returns, an older stamp's update can no longer run, and no caller can check
//! freshness and then lose a race before writing. There is no `is_current`.
//!
//! Every accepted change is one publication and moves the [`Revision`] that
//! [`Epoch::snapshot`] reports. Work that ends in failure commits through the
//! same stamp, so a stale failure never publishes either.
//!
//! Hold no guard across an `await`: reserve the stamp when the work is asked
//! for, compute the result outside, then commit it.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

/// State that newer work replaces, with the one identity allowed to commit.
pub struct Epoch<S> {
    shared: Arc<Mutex<Inner<S>>>,
}

struct Inner<S> {
    state: S,
    /// The identity whose stamp may commit; `None` once it committed or was retired.
    active: Option<u64>,
    /// The last identity handed out. Identities are never reused.
    issued: u64,
    revision: u64,
    open: bool,
}

/// The right to commit one piece of work, while nothing newer has begun.
///
/// Dropping a stamp abandons its work; it is never needed to release anything.
#[must_use = "work commits only through its stamp"]
pub struct Stamp<S> {
    shared: Arc<Mutex<Inner<S>>>,
    identity: u64,
}

/// The order of publications from one epoch. Only comparing them means anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(u64);

/// The state as last published, and which publication that was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation<S> {
    pub revision: Revision,
    pub state: S,
}

/// The epoch was closed, ran out of identities or revisions, or an update panicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("this owner is closed")]
pub struct Closed;

/// Newer work began, the work was retired, or the owner closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("newer work replaced this result")]
pub struct Stale;

/// Why [`Stamp::try_commit`] did not commit.
pub enum TryCommit<S, F> {
    Stale,
    /// Someone else held the state. The stamp and the update that did not run
    /// come back, to retry or drop.
    Busy(Stamp<S>, F),
}

impl<S> Epoch<S> {
    pub fn new(initial: S) -> Self {
        Self::starting(initial, 0, 0)
    }

    fn starting(state: S, issued: u64, revision: u64) -> Self {
        let inner = Inner {
            state,
            active: None,
            issued,
            revision,
            open: true,
        };
        Self {
            shared: Arc::new(Mutex::new(inner)),
        }
    }

    /// Begins work that replaces any outstanding work, applying `start` (such
    /// as showing it is loading) in the same turn.
    pub fn begin(&self, start: impl FnOnce(&mut S)) -> Result<Stamp<S>, Closed> {
        let mut inner = lock(&self.shared);
        if !inner.open {
            return Err(Closed);
        }
        let identity = inner.issued.checked_add(1);
        let (Some(identity), Some(_)) = (identity, inner.next_revision()) else {
            inner.close();
            return Err(Closed);
        };
        inner.issued = identity;
        inner.active = Some(identity);
        inner.publish(start);
        Ok(Stamp {
            shared: Arc::clone(&self.shared),
            identity,
        })
    }

    /// Retires outstanding work, applying `reset` in the same turn. A closed
    /// epoch stays as it is.
    pub fn invalidate(&self, reset: impl FnOnce(&mut S)) {
        let mut inner = lock(&self.shared);
        if !inner.open {
            return;
        }
        inner.active = None;
        if inner.next_revision().is_none() {
            inner.close();
            return;
        }
        inner.publish(reset);
    }

    /// Retires outstanding work for good: nothing begins or commits again.
    /// Reopening means a new epoch.
    pub fn close(&self) {
        lock(&self.shared).close();
    }

    pub fn snapshot(&self) -> Observation<S>
    where
        S: Clone,
    {
        let inner = lock(&self.shared);
        Observation {
            revision: Revision(inner.revision),
            state: inner.state.clone(),
        }
    }
}

impl<S> Stamp<S> {
    /// Applies this work's result, unless newer work began or the epoch closed.
    /// The update runs only when it is accepted.
    pub fn commit<R>(self, update: impl FnOnce(&mut S) -> R) -> Result<R, Stale> {
        let mut inner = lock(&self.shared);
        inner.accept(self.identity, update)
    }

    /// [`Self::commit`] without waiting for someone else's turn.
    pub fn try_commit<R, F: FnOnce(&mut S) -> R>(self, update: F) -> Result<R, TryCommit<S, F>> {
        let shared = Arc::clone(&self.shared);
        let mut inner = match shared.try_lock() {
            Ok(inner) => inner,
            Err(TryLockError::Poisoned(poisoned)) => broken(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => return Err(TryCommit::Busy(self, update)),
        };
        inner.accept(self.identity, update).map_err(|Stale| TryCommit::Stale)
    }
}

impl<S> Inner<S> {
    fn next_revision(&self) -> Option<u64> {
        self.revision.checked_add(1)
    }

    /// Callers have checked `next_revision`, under this same lock.
    fn publish<R>(&mut self, update: impl FnOnce(&mut S) -> R) -> R {
        let result = update(&mut self.state);
        self.revision += 1;
        result
    }

    fn accept<R>(&mut self, identity: u64, update: impl FnOnce(&mut S) -> R) -> Result<R, Stale> {
        if !self.open || self.active != Some(identity) {
            return Err(Stale);
        }
        self.active = None;
        if self.next_revision().is_none() {
            self.close();
            return Err(Stale);
        }
        Ok(self.publish(update))
    }

    fn close(&mut self) {
        self.open = false;
        self.active = None;
    }
}

/// An update that panicked may have left the state half changed: it stays
/// readable, and the epoch refuses all further work.
fn lock<S>(shared: &Mutex<Inner<S>>) -> MutexGuard<'_, Inner<S>> {
    shared.lock().unwrap_or_else(|poisoned| broken(poisoned.into_inner()))
}

fn broken<S>(mut inner: MutexGuard<'_, Inner<S>>) -> MutexGuard<'_, Inner<S>> {
    inner.close();
    inner
}

impl<S> fmt::Debug for Epoch<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Epoch")
    }
}

impl<S> fmt::Debug for Stamp<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Stamp")
    }
}

impl<S, F> fmt::Debug for TryCommit<S, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stale => "Stale",
            Self::Busy(..) => "Busy",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    /// What the state records: every publication, in order, by the token of
    /// the step that made it.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Entry {
        Started(u32),
        Finished(u32, bool),
        Reset(u32),
    }

    type Log = Vec<Entry>;

    #[test]
    fn a_newer_begin_retires_the_older_stamp() {
        let epoch = Epoch::new(0);
        let a = epoch.begin(|_| {}).unwrap();
        let b = epoch.begin(|_| {}).unwrap();
        assert_eq!(a.commit(|n| *n = 1), Err(Stale));
        assert_eq!(b.commit(|n| *n = 2), Ok(()));
        assert_eq!(epoch.snapshot().state, 2);
    }

    #[test]
    fn returning_to_the_same_request_is_a_third_identity() {
        // A, then B, then A again: the first A is not the current A.
        let epoch = Epoch::new("none");
        let first_a = epoch.begin(|s| *s = "loading a").unwrap();
        let b = epoch.begin(|s| *s = "loading b").unwrap();
        let second_a = epoch.begin(|s| *s = "loading a").unwrap();
        assert_eq!(first_a.commit(|s| *s = "old a"), Err(Stale));
        assert_eq!(b.commit(|s| *s = "b"), Err(Stale));
        assert_eq!(second_a.commit(|s| *s = "a"), Ok(()));
        assert_eq!(epoch.snapshot().state, "a");
    }

    #[test]
    fn a_stamp_commits_once_and_only_its_accepted_update_runs() {
        let epoch = Epoch::new(Vec::new());
        let stamp = epoch.begin(|_| {}).unwrap();
        epoch.invalidate(|log| log.push("reset"));
        let mut ran = false;
        assert_eq!(stamp.commit(|_| ran = true), Err(Stale));
        assert!(!ran, "a stale update never runs");
        assert_eq!(epoch.snapshot().state, ["reset"]);
    }

    #[test]
    fn closing_is_permanent() {
        let epoch = Epoch::new(0);
        let stamp = epoch.begin(|_| {}).unwrap();
        let before = epoch.snapshot();
        epoch.close();
        epoch.invalidate(|n| *n = 9);
        assert_eq!(stamp.commit(|n| *n = 1), Err(Stale));
        assert!(epoch.begin(|n| *n = 2).is_err());
        assert_eq!(epoch.snapshot(), before);
    }

    #[test]
    fn running_out_of_identities_closes_the_epoch() {
        let epoch = Epoch::starting(0, u64::MAX - 1, 0);
        let last = epoch.begin(|_| {}).unwrap();
        assert!(matches!(epoch.begin(|n| *n = 1), Err(Closed)));
        assert_eq!(last.commit(|n| *n = 2), Err(Stale));
        assert_eq!(epoch.snapshot().state, 0);
    }

    #[test]
    fn running_out_of_revisions_refuses_the_change_and_closes() {
        let epoch = Epoch::starting(0, 0, u64::MAX - 1);
        let stamp = epoch.begin(|n| *n = 1).unwrap();
        let mut ran = false;
        assert_eq!(stamp.commit(|_| ran = true), Err(Stale));
        assert!(!ran);
        assert!(epoch.begin(|_| {}).is_err());
        assert_eq!(epoch.snapshot().state, 1);
        assert_eq!(epoch.snapshot().revision, Revision(u64::MAX));
    }

    #[test]
    fn an_update_that_panics_closes_the_epoch() {
        let epoch = Epoch::new(Vec::new());
        let stamp = epoch.begin(|_| {}).unwrap();
        let panicked = thread::spawn(move || {
            stamp.commit(|log: &mut Vec<u8>| {
                log.push(1);
                panic!("an invented failure halfway through an update");
            })
        })
        .join();
        assert!(panicked.is_err());
        assert!(epoch.begin(|_| {}).is_err());
        // Half-done state stays readable, and frozen.
        assert_eq!(epoch.snapshot().state, [1]);
    }

    #[test]
    fn try_commit_hands_back_the_stamp_and_update_while_busy() {
        let epoch = Arc::new(Epoch::new(Vec::new()));
        let old = epoch.begin(|_| {}).unwrap();
        let (entered, wait_entered) = mpsc::channel();
        let (release, wait_release) = mpsc::channel::<()>();
        let holder = {
            let epoch = Arc::clone(&epoch);
            thread::spawn(move || {
                epoch.invalidate(|log| {
                    entered.send(()).unwrap();
                    wait_release.recv().unwrap();
                    log.push("reset");
                });
            })
        };
        wait_entered.recv().unwrap();
        let Err(TryCommit::Busy(old, update)) = old.try_commit(|log: &mut Vec<&str>| log.push("late")) else {
            panic!("the state was held, so the commit is busy");
        };
        release.send(()).unwrap();
        holder.join().unwrap();
        assert!(matches!(old.try_commit(update), Err(TryCommit::Stale)));
        assert_eq!(epoch.snapshot().state, ["reset"]);
    }

    /// A begin cannot pass between a commit's check and its update: once the
    /// newer begin returns, the older update has finished. A broken lock could
    /// let this pass by luck, never fail it.
    #[test]
    fn a_begin_waits_for_the_update_it_would_retire() {
        let epoch = Arc::new(Epoch::new(Log::new()));
        let a = epoch.begin(|log| log.push(Entry::Started(1))).unwrap();
        let (entered, wait_entered) = mpsc::channel();
        let committer = thread::spawn(move || {
            a.commit(|log| {
                entered.send(()).unwrap();
                thread::sleep(Duration::from_millis(20));
                log.push(Entry::Finished(1, true));
            })
        });
        wait_entered.recv().unwrap();
        let _b = epoch.begin(|log| log.push(Entry::Started(2))).unwrap();
        assert_eq!(
            epoch.snapshot().state,
            [Entry::Started(1), Entry::Finished(1, true), Entry::Started(2)]
        );
        assert_eq!(committer.join().unwrap(), Ok(()));
    }

    /// Threads begin, finish and retire work at once. Whatever the order, the
    /// log reads as one sequence where each finish follows its own start with
    /// nothing between them.
    #[test]
    fn contending_threads_publish_one_consistent_sequence() {
        let epoch = Arc::new(Epoch::new(Log::new()));
        let workers: Vec<_> = (0..4u32)
            .map(|worker| {
                let epoch = Arc::clone(&epoch);
                thread::spawn(move || {
                    for round in 0..500u32 {
                        let token = worker * 1000 + round;
                        if round % 7 == worker {
                            epoch.invalidate(|log| log.push(Entry::Reset(token)));
                            continue;
                        }
                        let stamp = epoch.begin(|log| log.push(Entry::Started(token))).unwrap();
                        thread::yield_now();
                        let _ = stamp.commit(|log| log.push(Entry::Finished(token, true)));
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let log = epoch.snapshot().state;
        for (at, entry) in log.iter().enumerate() {
            if let Entry::Finished(token, _) = entry {
                assert_eq!(
                    at.checked_sub(1).map(|before| log[before]),
                    Some(Entry::Started(*token))
                );
            }
        }
    }

    #[derive(Clone, Debug)]
    enum Action {
        Begin,
        /// Completes an outstanding stamp, chosen by position, from any epoch.
        Complete {
            pick: usize,
            ok: bool,
            try_first: bool,
        },
        Invalidate,
        Close,
        Reopen,
    }

    fn action() -> impl Strategy<Value = Action> {
        prop_oneof![
            4 => Just(Action::Begin),
            4 => (any::<usize>(), any::<bool>(), any::<bool>())
                .prop_map(|(pick, ok, try_first)| Action::Complete { pick, ok, try_first }),
            2 => Just(Action::Invalidate),
            1 => Just(Action::Close),
            1 => Just(Action::Reopen),
        ]
    }

    /// What one epoch should show, kept without stamps or identities.
    #[derive(Default)]
    struct Model {
        log: Log,
        /// The token of the one piece of work that may still finish.
        current: Option<u32>,
        closed: bool,
        publications: u64,
    }

    impl Model {
        fn publish(&mut self, entry: Entry) {
            self.log.push(entry);
            self.publications += 1;
        }
    }

    fn check(epochs: &[Epoch<Log>], models: &[Model], seen: &mut [Revision]) -> Result<(), TestCaseError> {
        for ((epoch, model), seen) in epochs.iter().zip(models).zip(seen) {
            let observed = epoch.snapshot();
            prop_assert_eq!(&observed.state, &model.log);
            prop_assert_eq!(observed.revision, Revision(model.publications));
            prop_assert!(observed.revision >= *seen);
            *seen = observed.revision;
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// Generated schedules against a sequential model, checked after every
        /// step: what each epoch shows, and which updates ran.
        #[test]
        fn schedules_match_the_sequential_model(actions in prop::collection::vec(action(), 0..48)) {
            let mut epochs = vec![Epoch::new(Log::new())];
            let mut models = vec![Model::default()];
            let mut seen = vec![Revision(0)];
            // (which epoch, token, stamp), completed in any order.
            let mut outstanding: Vec<(usize, u32, Stamp<Log>)> = Vec::new();
            for (step, action) in actions.into_iter().enumerate() {
                let token = u32::try_from(step).unwrap();
                let at = epochs.len() - 1;
                match action {
                    Action::Begin => {
                        let began = epochs[at].begin(|log| log.push(Entry::Started(token)));
                        let model = &mut models[at];
                        prop_assert_eq!(began.is_ok(), !model.closed);
                        if let Ok(stamp) = began {
                            model.publish(Entry::Started(token));
                            model.current = Some(token);
                            outstanding.push((at, token, stamp));
                        }
                    }
                    Action::Complete { pick, ok, try_first } => {
                        if outstanding.is_empty() {
                            continue;
                        }
                        let (owner, work, stamp) = outstanding.remove(pick % outstanding.len());
                        let mut ran = false;
                        let update = |log: &mut Log| {
                            ran = true;
                            log.push(Entry::Finished(work, ok));
                        };
                        let committed = if try_first {
                            match stamp.try_commit(update) {
                                Ok(()) => true,
                                Err(TryCommit::Stale) => false,
                                Err(TryCommit::Busy(..)) => panic!("nothing else holds the state"),
                            }
                        } else {
                            stamp.commit(update).is_ok()
                        };
                        let model = &mut models[owner];
                        let current = !model.closed && model.current == Some(work);
                        prop_assert_eq!(committed, current);
                        prop_assert_eq!(ran, current);
                        if current {
                            model.publish(Entry::Finished(work, ok));
                            model.current = None;
                        }
                    }
                    Action::Invalidate => {
                        let mut ran = false;
                        epochs[at].invalidate(|log| {
                            ran = true;
                            log.push(Entry::Reset(token));
                        });
                        let model = &mut models[at];
                        prop_assert_eq!(ran, !model.closed);
                        if !model.closed {
                            model.publish(Entry::Reset(token));
                            model.current = None;
                        }
                    }
                    Action::Close => {
                        epochs[at].close();
                        models[at].closed = true;
                        models[at].current = None;
                    }
                    Action::Reopen => {
                        epochs[at].close();
                        models[at].closed = true;
                        models[at].current = None;
                        epochs.push(Epoch::new(Log::new()));
                        models.push(Model::default());
                        seen.push(Revision(0));
                    }
                }
                check(&epochs, &models, &mut seen)?;
            }
        }
    }
}
