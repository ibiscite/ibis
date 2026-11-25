use crate::output::Formatted;
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
            if let Ok(mut state) = shared.state.lock() {
                if let Some(index) = state
                    .queue
                    .iter()
                    .position(|job| Arc::ptr_eq(&job.cancellation.cancelled, &self.cancelled))
                {
                    removed = state.queue.remove(index);
                    state.outstanding -= 1;
                }
                if let Some(active) = &state.active
                    && Arc::ptr_eq(&active.cancelled, &self.cancelled)
                {
                    // Release the caller without waiting for an uninterruptible
                    // native function to return to QuickJS's interrupt handler.
                    // The worker retains its capacity slot until it actually stops.
                    let _ = active.reply.send(Err(FormatError::Cancelled));
                }
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
    receiver: Option<mpsc::Receiver<Result<Formatted, FormatError>>>,
}
impl FormatTask {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    pub fn wait(self) -> Result<CitationResult, FormatError> {
        match self.formatted()? {
            Formatted::Citation(result) => Ok(result),
            Formatted::Document(_) => Err(FormatError::Runtime),
        }
    }
    pub(crate) fn formatted(mut self) -> Result<Formatted, FormatError> {
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
    reply: mpsc::Sender<Result<Formatted, FormatError>>,
}
#[derive(Default)]
struct State {
    queue: VecDeque<Job>,
    outstanding: usize,
    active: Option<Active>,
    foreground_seen: bool,
}
struct Active {
    cancelled: Arc<AtomicBool>,
    reply: mpsc::Sender<Result<Formatted, FormatError>>,
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
                    if let Some(active) = state.active.take() {
                        let _ = active.reply.send(Err(error));
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
    pub(crate) fn warm(&self, prepared: Prepared) -> Result<Option<FormatTask>, FormatError> {
        let mut state = self.shared.state.lock().map_err(|_| FormatError::Runtime)?;
        // Style loading can finish after the first preview. Never queue an obsolete
        // warm-up behind it, replacing the style the person is already using.
        if state.foreground_seen {
            return Ok(None);
        }
        self.enqueue(&mut state, prepared, Instant::now()).map(Some)
    }
    fn submit_with_clock(&self, prepared: Prepared, now: impl FnOnce() -> Instant) -> Result<FormatTask, FormatError> {
        let mut state = self.shared.state.lock().map_err(|_| FormatError::Runtime)?;
        let task = self.enqueue(&mut state, prepared, now())?;
        state.foreground_seen = true;
        Ok(task)
    }
    fn enqueue(&self, state: &mut State, prepared: Prepared, now: Instant) -> Result<FormatTask, FormatError> {
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
            deadline: now + self.limits.timeout,
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
                    state.active = Some(Active {
                        cancelled: job.cancellation.cancelled.clone(),
                        reply: job.reply.clone(),
                    });
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
            None if job.prepared.empty => Ok(crate::output::refused_formatted(&job.prepared)),
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
        state.active = None;
        let _ = job.reply.send(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::contract::{ItemOutcome, Notice};
    use serde_json::json;
    fn prepared(count: usize, bibliography: bool) -> Prepared {
        let items:Vec<_>=(0..count).map(|index|json!({"id":format!("ibis-{index}"),"type":"book","title":format!("Synthetic {index}"),"author":[{"family":format!("Invented{index}"),"given":"Avery"}],"issued":{"date-parts":[[2026]]},"publisher":"Imaginary Press"})).collect();
        let cites: Vec<_> = (0..count).map(|index| json!({"id":format!("ibis-{index}")})).collect();
        Prepared {json:json!({"style":"apa","items":items,"cites":cites,"bibliography":bibliography,"keep_order":false,"locale":null}).to_string(),xml:crate::catalog::resolve("apa").unwrap().xml.clone(),language:"en-US".into(),outcomes:(0..count).map(|item|ItemOutcome::Rendered {item}).collect(),losses:vec![],notices:vec![Notice::YearSuffixWithheld],warnings:vec![],empty:false,document:false}
    }
    fn hold_next_format(service: &Service) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered, running) = mpsc::channel();
        let (release, held) = mpsc::channel();
        *service.shared.format_gate.lock().unwrap() = Some((entered, held));
        (running, release)
    }
    #[test]
    fn running_request_cancels_and_restarts_then_next_request_succeeds() {
        let service = Service::new(Limits::default()).unwrap();
        service.submit(prepared(1, false)).unwrap().wait().unwrap();
        let (running, release) = hold_next_format(&service);
        let mut task = service.submit(prepared(1, true)).unwrap();
        running.recv_timeout(TEST_GATE_LIMIT).unwrap();
        task.cancel();
        // Cancellation releases the caller even if the processor is still
        // returning from a native function and has not reached its interrupt.
        let cancelled = task
            .receiver
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(1));
        release.send(()).unwrap();
        assert!(matches!(cancelled.unwrap(), Err(FormatError::Cancelled)));
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
}
