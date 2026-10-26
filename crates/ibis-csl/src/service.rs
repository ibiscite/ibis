use crate::{FormatError, Limits, Priority, input::Prepared, runtime::Session};
use ibis_core::contract::CitationResult;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Instant;

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
            if let Ok(mut state) = shared.state.lock() {
                for lane in 0..2 {
                    if let Some(index) = state.queues[lane]
                        .iter()
                        .position(|job| Arc::ptr_eq(&job.cancellation.cancelled, &self.cancelled))
                    {
                        removed = state.queues[lane].remove(index);
                        state.outstanding[lane] = state.outstanding[lane].saturating_sub(1);
                        break;
                    }
                }
                shared.interactive.store(
                    state.interactive_active || !state.queues[0].is_empty(),
                    Ordering::Release,
                );
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
    queues: [VecDeque<Job>; 2],
    outstanding: [usize; 2],
    interactive_active: bool,
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    stopping: Arc<AtomicBool>,
    interactive: Arc<AtomicBool>,
}
pub(crate) struct Service {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
    limits: Limits,
}
impl Service {
    pub(crate) fn new(limits: Limits) -> Result<Self, FormatError> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            stopping: Arc::new(AtomicBool::new(false)),
            interactive: Arc::new(AtomicBool::new(false)),
        });
        let mut service = Self {
            shared,
            threads: vec![],
            limits,
        };
        for lane in 0..2 {
            let shared = service.shared.clone();
            let limits = service.limits.clone();
            let handle = thread::Builder::new()
                .name(
                    if lane == 0 {
                        "ibis-csl-interactive"
                    } else {
                        "ibis-csl-background"
                    }
                    .into(),
                )
                .stack_size(4 * 1024 * 1024)
                .spawn(move || worker(shared, limits, lane))
                .map_err(|_| FormatError::Runtime)?;
            service.threads.push(handle);
        }
        Ok(service)
    }
    pub(crate) fn limits(&self) -> &Limits {
        &self.limits
    }
    pub(crate) fn submit(&self, prepared: Prepared, priority: Priority) -> Result<FormatTask, FormatError> {
        let lane = usize::from(priority == Priority::Background);
        let cap = if lane == 0 {
            self.limits.interactive_queue
        } else {
            self.limits.background_queue
        };
        let timeout = if lane == 0 {
            self.limits.interactive_timeout
        } else {
            self.limits.background_timeout
        };
        let mut state = self.shared.state.lock().map_err(|_| FormatError::Runtime)?;
        if self.shared.stopping.load(Ordering::Acquire) {
            return Err(FormatError::Runtime);
        }
        if state.outstanding[lane] >= cap {
            return Err(FormatError::Busy);
        }
        let cancellation = CancellationToken {
            cancelled: Arc::new(AtomicBool::new(false)),
            shared: Arc::downgrade(&self.shared),
        };
        let (reply, receiver) = mpsc::channel();
        state.queues[lane].push_back(Job {
            prepared,
            cancellation: cancellation.clone(),
            deadline: Instant::now() + timeout,
            reply,
        });
        state.outstanding[lane] += 1;
        if lane == 0 {
            self.shared.interactive.store(true, Ordering::Release);
        }
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
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
pub(crate) struct Control {
    pub cancel: Arc<AtomicBool>,
    pub stopping: Arc<AtomicBool>,
    pub interactive: Arc<AtomicBool>,
    pub deadline: Instant,
    pub background: bool,
}
impl Control {
    pub fn error(&self) -> Option<WorkError> {
        if self.cancel.load(Ordering::Acquire) || self.stopping.load(Ordering::Acquire) {
            Some(WorkError::Public(FormatError::Cancelled))
        } else if Instant::now() >= self.deadline {
            Some(WorkError::Public(FormatError::TimedOut))
        } else if self.background && self.interactive.load(Ordering::Acquire) {
            Some(WorkError::Preempted)
        } else {
            None
        }
    }
}
#[derive(Debug)]
pub(crate) enum WorkError {
    Public(FormatError),
    Preempted,
}
impl From<FormatError> for WorkError {
    fn from(error: FormatError) -> Self {
        Self::Public(error)
    }
}

fn worker(shared: Arc<Shared>, limits: Limits, lane: usize) {
    let mut session = None;
    loop {
        let job = {
            let Ok(mut state) = shared.state.lock() else { return };
            loop {
                if shared.stopping.load(Ordering::Acquire) {
                    for queue in &mut state.queues {
                        for job in queue.drain(..) {
                            let _ = job.reply.send(Err(FormatError::Cancelled));
                        }
                    }
                    return;
                }
                let disposable = state.queues[lane].front().is_some_and(|job| {
                    job.cancellation.cancelled.load(Ordering::Acquire) || Instant::now() >= job.deadline
                });
                if !state.queues[lane].is_empty()
                    && (lane == 0 || disposable || (!state.interactive_active && state.queues[0].is_empty()))
                {
                    break;
                }
                state = if let Some(job) = state.queues[lane].front() {
                    let delay = job.deadline.saturating_duration_since(Instant::now());
                    match shared.wake.wait_timeout(state, delay) {
                        Ok((state, _)) => state,
                        Err(_) => return,
                    }
                } else {
                    match shared.wake.wait(state) {
                        Ok(state) => state,
                        Err(_) => return,
                    }
                };
            }
            if lane == 0 {
                state.interactive_active = true;
            }
            let Some(job) = state.queues[lane].pop_front() else {
                return;
            };
            job
        };
        let control = Control {
            cancel: job.cancellation.cancelled.clone(),
            stopping: shared.stopping.clone(),
            interactive: shared.interactive.clone(),
            deadline: job.deadline,
            background: lane == 1,
        };
        let result = match control.error() {
            Some(error) => Err(error),
            None if job.prepared.empty => Ok(crate::output::refused(&job.prepared)),
            None => {
                if session.is_none() {
                    match Session::new(&limits, &control) {
                        Ok(value) => session = Some(value),
                        Err(error) => {
                            finish(&shared, lane, job, Err(error));
                            continue;
                        }
                    }
                }
                match session.as_mut() {
                    Some(session) => session.run(&job.prepared, &limits, &control),
                    None => Err(FormatError::Runtime.into()),
                }
            }
        };
        if result.is_err() {
            session = None;
        }
        finish(&shared, lane, job, result);
    }
}
fn finish(shared: &Shared, lane: usize, job: Job, result: Result<CitationResult, WorkError>) {
    let Ok(mut state) = shared.state.lock() else { return };
    match result {
        Err(WorkError::Preempted) => state.queues[lane].push_front(job),
        result => {
            state.outstanding[lane] = state.outstanding[lane].saturating_sub(1);
            let _ = job.reply.send(result.map_err(|error| match error {
                WorkError::Public(error) => error,
                WorkError::Preempted => FormatError::Runtime,
            }));
        }
    }
    if lane == 0 {
        state.interactive_active = false;
        shared.interactive.store(!state.queues[0].is_empty(), Ordering::Release);
    }
    shared.wake.notify_all();
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use ibis_core::contract::{ItemOutcome, Notice};
    use serde_json::json;
    use std::time::Duration;
    fn prepared(count: usize, bibliography: bool) -> Prepared {
        let items:Vec<_>=(0..count).map(|index|json!({"id":format!("ibis-{index}"),"type":"book","title":format!("Synthetic {index}"),"author":[{"family":format!("Invented{index}"),"given":"Avery"}],"issued":{"date-parts":[[2026]]},"publisher":"Imaginary Press"})).collect();
        let cites: Vec<_> = (0..count).map(|index| json!({"id":format!("ibis-{index}")})).collect();
        Prepared {json:json!({"style":"apa","items":items,"cites":cites,"bibliography":bibliography,"keep_order":false,"locale":null}).to_string(),outcomes:(0..count).map(|item|ItemOutcome::Rendered {item}).collect(),losses:vec![],notices:vec![Notice::YearSuffixWithheld],empty:false}
    }
    #[test]
    fn queue_is_bounded_and_cancelled_request_does_not_poison_worker() {
        let service = Service::new(Limits {
            background_queue: 1,
            ..Limits::default()
        })
        .unwrap();
        {
            let mut state = service.shared.state.lock().unwrap();
            state.interactive_active = true;
            service.shared.interactive.store(true, Ordering::Release);
        }
        let first = service.submit(prepared(1, true), Priority::Background).unwrap();
        assert!(matches!(
            service.submit(prepared(1, true), Priority::Background),
            Err(FormatError::Busy)
        ));
        first.cancel();
        {
            let mut state = service.shared.state.lock().unwrap();
            state.interactive_active = false;
            service.shared.interactive.store(false, Ordering::Release);
            service.shared.wake.notify_all();
        }
        assert_eq!(first.wait(), Err(FormatError::Cancelled));
        assert!(
            service
                .submit(prepared(1, true), Priority::Background)
                .unwrap()
                .wait()
                .unwrap()
                .copy_content(1)
                .is_some()
        );
    }
    #[test]
    fn running_request_cancels_and_restarts_then_next_request_succeeds() {
        let service = Service::new(Limits::default()).unwrap();
        service
            .submit(prepared(1, false), Priority::Interactive)
            .unwrap()
            .wait()
            .unwrap();
        let task = service.submit(prepared(100, true), Priority::Interactive).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !service.shared.state.lock().unwrap().interactive_active && Instant::now() < deadline {
            thread::yield_now();
        }
        thread::sleep(Duration::from_millis(20));
        task.cancel();
        assert_eq!(task.wait(), Err(FormatError::Cancelled));
        assert!(
            service
                .submit(prepared(1, false), Priority::Interactive)
                .unwrap()
                .wait()
                .unwrap()
                .copy_content(1)
                .is_some()
        );
    }
    #[test]
    fn interactive_request_preempts_background_without_losing_it() {
        let service = Service::new(Limits::default()).unwrap();
        let background = service.submit(prepared(100, true), Priority::Background).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !service.shared.state.lock().unwrap().queues[1].is_empty() && Instant::now() < deadline {
            thread::yield_now();
        }
        let interactive = service.submit(prepared(1, false), Priority::Interactive).unwrap();
        assert!(interactive.wait().unwrap().copy_content(1).is_some());
        assert!(background.wait().unwrap().copy_content(100).is_some());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod waiting_tests {
    use super::*;
    use std::time::Duration;
    fn empty_job() -> Prepared {
        Prepared {
            json: String::new(),
            outcomes: vec![],
            losses: vec![],
            notices: vec![],
            empty: true,
        }
    }
    #[test]
    fn cancelling_queued_background_work_does_not_wait_for_interactive_idle() {
        let service = Service::new(Limits::default()).unwrap();
        service.shared.state.lock().unwrap().interactive_active = true;
        service.shared.interactive.store(true, Ordering::Release);
        let mut task = service.submit(empty_job(), Priority::Background).unwrap();
        task.cancel();
        assert_eq!(
            task.receiver
                .take()
                .unwrap()
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
            Err(FormatError::Cancelled)
        );
    }
    #[test]
    fn queued_background_deadline_expires_while_interactive_remains_active() {
        let service = Service::new(Limits {
            background_timeout: Duration::from_millis(10),
            ..Limits::default()
        })
        .unwrap();
        service.shared.state.lock().unwrap().interactive_active = true;
        service.shared.interactive.store(true, Ordering::Release);
        let mut task = service.submit(empty_job(), Priority::Background).unwrap();
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
