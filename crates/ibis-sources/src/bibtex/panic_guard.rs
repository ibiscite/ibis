//! Keep contained parser panics private, forwarding unrelated panics unchanged.
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

static INSTALL: Once = Once::new();
thread_local! {
    static SUPPRESS: Cell<bool> = const { Cell::new(false) };
}

struct Scope(bool);
impl Drop for Scope {
    fn drop(&mut self) {
        let _ = SUPPRESS.try_with(|flag| flag.set(self.0));
    }
}

pub(super) fn attempt<T>(read: impl FnOnce() -> T) -> std::thread::Result<T> {
    // A prior hook or an unwinding destructor may reenter the loader. Never
    // mutate hooks or wait on our own Once while already handling a panic.
    if std::thread::panicking() {
        return Err(Box::new(()));
    }
    INSTALL.call_once(|| {
        // Stable take_hook/set_hook has an installation gap. Once coordinates
        // our parser calls, not external hook writers; a later set_hook replaces
        // this dispatcher. Do not claim atomic coexistence with external writers.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !SUPPRESS.try_with(Cell::get).unwrap_or(false) {
                previous(info);
            }
        }));
    });
    match catch_unwind(AssertUnwindSafe(|| {
        let previous = SUPPRESS.try_with(|flag| flag.replace(true)).ok()?;
        let _scope = Scope(previous);
        Some(read())
    })) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(Box::new(())), // TLS unavailable: do not execute unguarded.
        Err(payload) => Err(payload),
    }
}

#[cfg(test)]
mod tests {
    use super::attempt;
    use std::panic::{catch_unwind, panic_any, set_hook};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    const PRIVATE: &str = "SYNTHETIC PRIVATE PARSER PANIC";
    const UNRELATED: &str = "synthetic unrelated panic";
    const CHILD: &str = "IBIS_PARSER_HOOK_CHILD";

    fn private(result: std::thread::Result<()>) {
        let Err(payload) = result else {
            panic!("parser did not panic")
        };
        assert_eq!(payload.downcast_ref::<&str>(), Some(&PRIVATE));
    }

    fn normal() {
        let observed = Arc::new(AtomicUsize::new(0));
        let hook_observed = observed.clone();
        set_hook(Box::new(move |info| {
            hook_observed.fetch_add(1, Ordering::SeqCst);
            eprintln!(
                "{}",
                info.payload().downcast_ref::<&str>().unwrap_or(&"unexpected panic")
            );
        }));
        let active = Arc::new(Barrier::new(9));
        let resume = Arc::new(Barrier::new(9));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let (active, resume) = (active.clone(), resume.clone());
                std::thread::spawn(move || {
                    attempt(|| {
                        active.wait();
                        resume.wait();
                        panic_any(PRIVATE);
                    })
                })
            })
            .collect();
        active.wait();
        assert!(catch_unwind(|| panic_any(UNRELATED)).is_err());
        resume.wait();
        for worker in workers {
            private(
                worker
                    .join()
                    .unwrap_or_else(|payload| std::panic::resume_unwind(payload)),
            );
        }
        let mut inner = None;
        let outer = attempt(|| {
            inner = Some(attempt(|| panic_any(PRIVATE)));
            panic_any(PRIVATE);
        });
        private(outer);
        private(inner.unwrap_or_else(|| panic!("nested parser did not run")));
        assert_eq!(attempt(|| 7).ok(), Some(7));
        assert!(catch_unwind(|| panic_any(UNRELATED)).is_err());
        assert_eq!(observed.load(Ordering::SeqCst), 2);
    }

    fn reentry() {
        let ran = Arc::new(AtomicUsize::new(0));
        let refused = Arc::new(AtomicUsize::new(0));
        let (hook_ran, hook_refused) = (ran.clone(), refused.clone());
        set_hook(Box::new(move |_| {
            if attempt(|| {
                hook_ran.fetch_add(1, Ordering::SeqCst);
            })
            .is_err()
            {
                hook_refused.fetch_add(1, Ordering::SeqCst);
            }
        }));
        for expected in 1..=2 {
            assert!(catch_unwind(|| panic_any(UNRELATED)).is_err());
            assert_eq!(refused.load(Ordering::SeqCst), expected);
            assert_eq!(ran.load(Ordering::SeqCst), 0);
            assert!(attempt(|| ()).is_ok());
        }
    }

    #[test]
    fn parser_hooks_in_subprocesses() {
        if let Ok(mode) = std::env::var(CHILD) {
            match mode.as_str() {
                "normal" => normal(),
                "reentry" => reentry(),
                "abort" => {
                    set_hook(Box::new(|_| std::process::abort()));
                    private(attempt(|| panic_any(PRIVATE)));
                    eprintln!("private parser survived");
                    panic_any(UNRELATED);
                }
                _ => panic!("unknown child mode"),
            }
            return;
        }
        for mode in ["normal", "reentry", "abort"] {
            let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let mut child = Command::new(std::env::current_exe().unwrap_or_else(|e| panic!("{e}")))
                .args([
                    "--exact",
                    "bibtex::panic_guard::tests::parser_hooks_in_subprocesses",
                    "--nocapture",
                ])
                .env(CHILD, mode)
                .current_dir(dir.path())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap_or_else(|e| panic!("{e}"));
            let started = Instant::now();
            while child.try_wait().unwrap_or_else(|e| panic!("{e}")).is_none() {
                if started.elapsed() > Duration::from_secs(10) {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("{mode} child timed out");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let output = child.wait_with_output().unwrap_or_else(|e| panic!("{e}"));
            let stderr = String::from_utf8_lossy(&output.stderr);
            if mode == "abort" {
                assert!(!output.status.success());
                assert_ne!(
                    output.status.code(),
                    Some(101),
                    "unrelated panic must abort, not unwind"
                );
                assert_eq!(stderr, "private parser survived\n");
            } else {
                assert!(output.status.success(), "{mode}: {stderr}");
                let expected = if mode == "normal" {
                    format!("{UNRELATED}\n{UNRELATED}\n")
                } else {
                    String::new()
                };
                assert_eq!(stderr, expected);
            }
        }
    }
}
