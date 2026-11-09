use crate::{FormatError, Limits, input::Prepared, runtime::Session};
use ibis_core::contract::CitationResult;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Instant;

#[cfg(test)]
const TEST_GATE_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    shared: Weak<Shared>,
}
impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(shared) = self.shared.upgrade() {
            let mut removed = None;
            if let Ok(mut state) = shared.state.lock()
                && let Some(index) = state
                    .queue
                    .iter()
                    .position(|job| Arc::ptr_eq(&job.cancellation.cancelled, &self.cancelled))
            {
                removed = state.queue.remove(index);
                state.outstanding -= 1;
            }
            shared.wake.notify_all();
            if let Some(job) = removed {
                let _ = job.reply.send(Err(FormatError::Cancelled));
            }
        }
    }
}

pub struct FormatTask {
    cancellation: CancellationToken,
    receiver: Option<mpsc::Receiver<Result<CitationResult, FormatError>>>,
}
impl FormatTask {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    pub fn wait(mut self) -> Result<CitationResult, FormatError> {
        self.receiver
            .take()
            .ok_or(FormatError::Runtime)?
            .recv()
            .map_err(|_| FormatError::Runtime)?
    }
}
impl Drop for FormatTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(crate) struct Job {
    prepared: Prepared,
    cancellation: CancellationToken,
    deadline: Instant,
    reply: mpsc::Sender<Result<CitationResult, FormatError>>,
}
#[derive(Default)]
struct State {
    queue: VecDeque<Job>,
    outstanding: usize,
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    stopping: Arc<AtomicBool>,
    #[cfg(test)]
    format_gate: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
}
pub(crate) struct Service {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    limits: Limits,
}
impl Service {
    pub(crate) fn new(limits: Limits) -> Result<Self, FormatError> {
        let mut service = Self {
            shared: Arc::default(),
            thread: None,
            limits,
        };
        service.start_worker()?;
        Ok(service)
    }
    fn start_worker(&mut self) -> Result<(), FormatError> {
        let shared = self.shared.clone();
        let limits = self.limits.clone();
        self.thread = Some(
            thread::Builder::new()
                .name("ibis-csl".into())
                .stack_size(crate::WORKER_STACK_BYTES)
                .spawn(move || {
                    // A panic must also release queued callers and refuse new work.
                    let _ = std::panic::catch_unwind(|| worker(&shared, &limits));
                    let mut state = shared.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    let error = if shared.stopping.swap(true, Ordering::AcqRel) {
                        FormatError::Cancelled
                    } else {
                        FormatError::Runtime
                    };
                    for job in state.queue.drain(..) {
                        let _ = job.reply.send(Err(error));
                    }
                    state.outstanding = 0;
                })
                .map_err(|_| FormatError::Runtime)?,
        );
        Ok(())
    }
    pub(crate) fn limits(&self) -> &Limits {
        &self.limits
    }
    pub(crate) fn submit(&self, prepared: Prepared) -> Result<FormatTask, FormatError> {
        self.submit_with_clock(prepared, Instant::now)
    }
    fn submit_with_clock(&self, prepared: Prepared, now: impl FnOnce() -> Instant) -> Result<FormatTask, FormatError> {
        let mut state = self.shared.state.lock().map_err(|_| FormatError::Runtime)?;
        if self.shared.stopping.load(Ordering::Acquire) {
            return Err(FormatError::Runtime);
        }
        if state.outstanding >= self.limits.queue {
            return Err(FormatError::Busy);
        }
        let cancellation = CancellationToken {
            cancelled: Arc::new(AtomicBool::new(false)),
            shared: Arc::downgrade(&self.shared),
        };
        let (reply, receiver) = mpsc::channel();
        state.queue.push_back(Job {
            prepared,
            cancellation: cancellation.clone(),
            deadline: now() + self.limits.timeout,
            reply,
        });
        state.outstanding += 1;
        self.shared.wake.notify_all();
        Ok(FormatTask {
            cancellation,
            receiver: Some(receiver),
        })
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        {
            // Change the condition under the same mutex as Condvar::wait;
            // otherwise an idle worker can miss the shutdown notification.
            let _state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            self.shared.stopping.store(true, Ordering::Release);
            self.shared.wake.notify_all();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
pub(crate) struct Control {
    pub cancel: Arc<AtomicBool>,
    pub stopping: Arc<AtomicBool>,
    pub deadline: Instant,
}
impl Control {
    pub fn error(&self) -> Option<FormatError> {
        if self.cancel.load(Ordering::Acquire) || self.stopping.load(Ordering::Acquire) {
            Some(FormatError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Some(FormatError::TimedOut)
        } else {
            None
        }
    }
}
fn worker(shared: &Shared, limits: &Limits) {
    let mut session = None;
    loop {
        let job = {
            let Ok(mut state) = shared.state.lock() else { return };
            loop {
                if shared.stopping.load(Ordering::Acquire) {
                    return;
                }
                if let Some(job) = state.queue.pop_front() {
                    break job;
                }
                state = match shared.wake.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
        };
        let control = Control {
            cancel: job.cancellation.cancelled.clone(),
            stopping: shared.stopping.clone(),
            deadline: job.deadline,
        };
        let result = match control.error() {
            Some(error) => Err(error),
            None if job.prepared.empty => Ok(crate::output::refused(&job.prepared)),
            None => {
                #[cfg(test)]
                if let Some((entered, release)) = shared.format_gate.lock().ok().and_then(|mut gate| gate.take()) {
                    let _ = entered.send(());
                    let _ = release.recv_timeout(TEST_GATE_LIMIT);
                }
                Session::format(&mut session, &job.prepared, limits, &control)
            }
        };
        if result.is_err() {
            session = None;
        }
        let Ok(mut state) = shared.state.lock() else { return };
        state.outstanding -= 1;
        let _ = job.reply.send(result);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use ibis_core::contract::{ItemOutcome, Notice};
    use serde_json::json;
    pub(super) fn queued_service(limits: Limits) -> Service {
        Service {
            shared: Arc::default(),
            thread: None,
            limits,
        }
    }
    fn prepared(count: usize, bibliography: bool) -> Prepared {
        let items:Vec<_>=(0..count).map(|index|json!({"id":format!("ibis-{index}"),"type":"book","title":format!("Synthetic {index}"),"author":[{"family":format!("Invented{index}"),"given":"Avery"}],"issued":{"date-parts":[[2026]]},"publisher":"Imaginary Press"})).collect();
        let cites: Vec<_> = (0..count).map(|index| json!({"id":format!("ibis-{index}")})).collect();
        Prepared {json:json!({"style":"apa","items":items,"cites":cites,"bibliography":bibliography,"keep_order":false,"locale":null}).to_string(),xml:crate::catalog::resolve("apa").unwrap().xml.clone(),language:"en-US".into(),outcomes:(0..count).map(|item|ItemOutcome::Rendered {item}).collect(),losses:vec![],notices:vec![Notice::YearSuffixWithheld],warnings:vec![],empty:false}
    }
    fn hold_next_format(service: &Service) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered, running) = mpsc::channel();
        let (release, held) = mpsc::channel();
        *service.shared.format_gate.lock().unwrap() = Some((entered, held));
        (running, release)
    }
    #[test]
    fn cancelling_queued_request_frees_its_slot_and_worker_recovers() {
        let mut service = queued_service(Limits {
            queue: 1,
            ..Limits::default()
        });
        let first = service.submit(prepared(1, true)).unwrap();
        assert!(matches!(service.submit(prepared(1, true)), Err(FormatError::Busy)));
        first.cancellation().cancel();
        assert_eq!(first.wait(), Err(FormatError::Cancelled));
        drop(service.submit(prepared(1, true)).unwrap());
        let replacement = service.submit(prepared(1, true)).unwrap();
        service.start_worker().unwrap();
        assert!(replacement.wait().unwrap().copy_content(1).is_some());
    }
    #[test]
    fn running_request_cancels_and_restarts_then_next_request_succeeds() {
        let service = Service::new(Limits::default()).unwrap();
        service.submit(prepared(1, false)).unwrap().wait().unwrap();
        let (running, release) = hold_next_format(&service);
        let task = service.submit(prepared(1, true)).unwrap();
        running.recv_timeout(TEST_GATE_LIMIT).unwrap();
        task.cancel();
        release.send(()).unwrap();
        assert_eq!(task.wait(), Err(FormatError::Cancelled));
        assert!(
            service
                .submit(prepared(1, false))
                .unwrap()
                .wait()
                .unwrap()
                .copy_content(1)
                .is_some()
        );
    }
    #[test]
    fn shutdown_cancels_running_and_queued_requests() {
        let service = Service::new(Limits::default()).unwrap();
        let (entered, release) = hold_next_format(&service);
        let running = service.submit(prepared(1, true)).unwrap();
        entered.recv_timeout(TEST_GATE_LIMIT).unwrap();
        let queued = service.submit(prepared(1, false)).unwrap();
        let shared = service.shared.clone();
        let shutdown = thread::spawn(move || drop(service));
        {
            let state = shared.state.lock().unwrap();
            let (_state, _) = shared
                .wake
                .wait_timeout_while(state, TEST_GATE_LIMIT, |_| !shared.stopping.load(Ordering::Acquire))
                .unwrap();
            assert!(shared.stopping.load(Ordering::Acquire));
        }
        release.send(()).unwrap();
        shutdown.join().unwrap();
        assert_eq!(running.wait(), Err(FormatError::Cancelled));
        assert_eq!(queued.wait(), Err(FormatError::Cancelled));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod waiting_tests {
    use super::tests::queued_service;
    use super::*;
    use std::time::Duration;
    fn empty_job() -> Prepared {
        Prepared {
            json: String::new(),
            xml: "".into(),
            language: "en-US".into(),
            outcomes: vec![],
            losses: vec![],
            notices: vec![],
            warnings: vec![],
            empty: true,
        }
    }
    #[test]
    fn worker_failure_releases_waiters_and_refuses_new_work() {
        let mut service = queued_service(Limits::default());
        let mut task = service.submit(empty_job()).unwrap();
        let shared = service.shared.clone();
        assert!(
            thread::spawn(move || {
                let _state = shared.state.lock().unwrap();
                panic!("synthetic scheduler failure");
            })
            .join()
            .is_err()
        );
        service.start_worker().unwrap();
        assert_eq!(
            task.receiver
                .take()
                .unwrap()
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
            Err(FormatError::Runtime)
        );
        service.shared.state.clear_poison();
        assert!(matches!(service.submit(empty_job()), Err(FormatError::Runtime)));
    }
    #[test]
    fn queued_deadline_includes_time_waiting_for_worker() {
        let timeout = Duration::from_secs(1);
        let mut service = queued_service(Limits {
            timeout,
            ..Limits::default()
        });
        let mut task = service
            .submit_with_clock(empty_job(), || Instant::now() - timeout)
            .unwrap();
        service.start_worker().unwrap();
        assert_eq!(
            task.receiver
                .take()
                .unwrap()
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
            Err(FormatError::TimedOut)
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod shutdown_tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn shutdown_cannot_change_wait_predicate_outside_its_mutex() {
        let service = Service::new(Limits::default()).unwrap();
        let shared = service.shared.clone();
        let predicate = shared.state.lock().unwrap();
        let (started, ready) = mpsc::channel();
        let join = thread::spawn(move || {
            started.send(()).unwrap();
            drop(service);
        });
        ready.recv().unwrap();
        let deadline = Instant::now() + Duration::from_millis(100);
        while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(
            !shared.stopping.load(Ordering::Acquire),
            "shutdown must acquire the wait predicate lock before changing it"
        );
        drop(predicate);
        join.join().unwrap();
    }
}
