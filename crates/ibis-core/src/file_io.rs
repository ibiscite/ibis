//! Bounded waits for read-only filesystem work. A stalled driver keeps its
//! permit until it returns; cancellation and deadlines discard late answers.
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    TimedOut,
    Capacity,
    Worker,
}

/// Capacity policy belongs to the caller. Optional work cannot consume the
/// slots reserved for required reads, and never queues behind them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Wait,
    Immediate,
    Optional,
}

pub struct Scope<'a> {
    pub until: Instant,
    pub now: Arc<dyn Fn() -> Instant + Send + Sync>,
    pub cancelled: &'a dyn Fn() -> bool,
}

impl<'a> Scope<'a> {
    pub fn new(cancelled: &'a dyn Fn() -> bool, duration: Duration) -> Self {
        Self {
            until: Instant::now() + duration,
            now: Arc::new(Instant::now),
            cancelled,
        }
    }

    pub fn remaining(&self) -> Result<Duration, Error> {
        if (self.cancelled)() {
            return Err(Error::Cancelled);
        }
        let remaining = self.until.saturating_duration_since((self.now)());
        if remaining.is_zero() {
            Err(Error::TimedOut)
        } else {
            Ok(remaining)
        }
    }
}

#[derive(Default)]
struct InFlight {
    total: usize,
    optional: usize,
}

#[derive(Clone)]
pub struct Pool {
    state: Arc<(Mutex<InFlight>, Condvar)>,
    limit: usize,
    optional_limit: usize,
}

struct Permit(Pool, Admission);
impl Drop for Permit {
    fn drop(&mut self) {
        let (count, available) = &*self.0.state;
        let mut count = count.lock().unwrap_or_else(|e| e.into_inner());
        count.total -= 1;
        if self.1 == Admission::Optional {
            count.optional -= 1;
        }
        available.notify_one();
    }
}

impl Pool {
    pub fn new(limit: usize, optional_limit: usize) -> Self {
        Self {
            state: Arc::default(),
            limit,
            optional_limit,
        }
    }

    pub fn run<T: Send + 'static>(
        &self,
        scope: &Scope<'_>,
        admission: Admission,
        operation: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, Error> {
        let (count, available) = &*self.state;
        let mut count = count.lock().map_err(|_| Error::Worker)?;
        scope.remaining()?;
        if (admission == Admission::Optional && count.optional >= self.optional_limit)
            || (admission != Admission::Wait && count.total >= self.limit)
        {
            return Err(Error::Capacity);
        }
        while count.total >= self.limit {
            let remaining = scope.remaining()?;
            count = available
                .wait_timeout(count, remaining.min(Duration::from_millis(5)))
                .map_err(|_| Error::Worker)?
                .0;
        }
        scope.remaining()?;
        count.total += 1;
        if admission == Admission::Optional {
            count.optional += 1;
        }
        drop(count);
        let permit = Permit(self.clone(), admission);
        let (send, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("ibis-file-io".into())
            .spawn(move || {
                let _permit = permit;
                let _ = send.send(operation());
            })
            .map_err(|_| Error::Worker)?;
        loop {
            let remaining = scope.remaining()?;
            match result.recv_timeout(remaining.min(Duration::from_millis(5))) {
                Ok(answer) => {
                    scope.remaining()?;
                    return Ok(answer);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(Error::Worker),
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn stalled_work_keeps_capacity_until_it_finishes() {
        let pool = Pool::new(1, 0);
        let (release, held) = mpsc::channel::<()>();
        assert_eq!(
            pool.run(
                &Scope::new(&|| false, Duration::from_millis(20)),
                Admission::Wait,
                move || held.recv().ok()
            ),
            Err(Error::TimedOut)
        );
        let scope = Scope::new(&|| false, Duration::from_secs(1));
        assert_eq!(pool.run(&scope, Admission::Immediate, || 1), Err(Error::Capacity));
        release.send(()).unwrap();
        assert_eq!(pool.run(&scope, Admission::Wait, || 2), Ok(2));
    }
    #[test]
    fn late_answers_are_discarded_when_cancelled() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let check = || cancelled.load(Ordering::Acquire);
        assert_eq!(
            Pool::new(1, 0).run(
                &Scope::new(&check, Duration::from_secs(1)),
                Admission::Wait,
                move || {
                    signal.store(true, Ordering::Release);
                    42
                }
            ),
            Err(Error::Cancelled)
        );
    }
}
