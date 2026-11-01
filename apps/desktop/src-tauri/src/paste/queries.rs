#[cfg(any(target_os = "windows", test))]
use std::sync::Mutex;
#[cfg(any(target_os = "windows", test))]
use std::time::Duration;

/// One in-flight native query, with no queue. A timed-out worker retains its
/// permit until it actually exits; busy callers immediately use their fallback.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
pub(super) struct QueryWorker(pub(super) std::sync::Arc<std::sync::atomic::AtomicBool>);

#[cfg(any(target_os = "windows", test))]
struct QueryPermit(std::sync::Arc<std::sync::atomic::AtomicBool>);

#[cfg(any(target_os = "windows", test))]
impl Drop for QueryPermit {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(any(target_os = "windows", test))]
impl QueryWorker {
    pub(super) fn run<T: Send + 'static>(
        &self,
        limit: Duration,
        query: impl FnOnce() -> T + Send + 'static,
    ) -> Option<T> {
        self.0
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::Acquire,
                std::sync::atomic::Ordering::Relaxed,
            )
            .ok()?;
        run_query(QueryPermit(self.0.clone()), limit, query)
    }
}

/// A stuck provider blocks only its target process. No queue or idle entries;
/// at most four native queries may remain outstanding across all processes.
#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
pub(super) struct PasswordQueries(pub(super) std::sync::Arc<Mutex<Vec<i32>>>);

#[cfg(any(target_os = "windows", test))]
const PASSWORD_QUERY_LIMIT: usize = 4;

#[cfg(any(target_os = "windows", test))]
struct PasswordPermit {
    active: std::sync::Arc<Mutex<Vec<i32>>>,
    pid: i32,
}

#[cfg(any(target_os = "windows", test))]
impl Drop for PasswordPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.retain(|pid| *pid != self.pid);
        }
    }
}

#[cfg(any(target_os = "windows", test))]
impl PasswordQueries {
    pub(super) fn run<T: Send + 'static>(
        &self,
        pid: i32,
        limit: Duration,
        query: impl FnOnce() -> T + Send + 'static,
    ) -> Option<T> {
        let mut active = self.0.lock().ok()?;
        if active.len() >= PASSWORD_QUERY_LIMIT || active.contains(&pid) {
            return None;
        }
        active.push(pid);
        drop(active);
        run_query(
            PasswordPermit {
                active: self.0.clone(),
                pid,
            },
            limit,
            query,
        )
    }
}

#[cfg(any(target_os = "windows", test))]
fn run_query<T: Send + 'static>(
    permit: impl Send + 'static,
    limit: Duration,
    query: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .spawn(move || {
            let _permit = permit;
            let _ = sender.send(query());
        })
        .ok()?;
    receiver.recv_timeout(limit).ok()
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn display_name(
    worker: &QueryWorker,
    executable: String,
    query: impl FnOnce() -> Option<String> + Send + 'static,
) -> String {
    worker
        .run(Duration::from_millis(50), query)
        .flatten()
        .unwrap_or(executable)
}
