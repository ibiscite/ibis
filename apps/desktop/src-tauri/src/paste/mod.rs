//! Enter pastes into the app the picker was opened from (#27, #1241).
//!
//! After a clean copy has hidden the picker, Ibis posts one ⌘V, only to the
//! captured process, once that app is in front again and the clipboard still
//! holds the citation. `ibis_core::destination::decide` says when it must only
//! copy, and the notice says why. The citation stays on the clipboard either
//! way (ADR 0009).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ibis_core::destination::{AppId, CopyReason, Decision, PastePermission, System, decide, lookup};
use serde::Serialize;
use tauri::{AppHandle, Manager};

mod capture;
#[cfg(target_os = "macos")]
mod macos;
pub(crate) mod permission;
#[cfg(any(target_os = "macos", test))]
mod security;

use capture::{Captured, Front};

const SYSTEM: System = if cfg!(target_os = "macos") {
    System::Macos
} else if cfg!(target_os = "windows") {
    System::Windows
} else {
    System::Linux
};

/// How long the captured app gets to come back to the front after the picker
/// hides, in checks and in real time; a paste later than `LATE` never happens.
const RETURN_LIMIT: Duration = Duration::from_millis(1_000);
const RETURN_STEP: Duration = Duration::from_millis(25);
const RETURN_CHECKS: u32 = 40;
const LATE: Duration = Duration::from_millis(1_500);
/// Time for the app to give its window the keyboard again.
const SETTLE: Duration = Duration::from_millis(50);

/// An unresponsive accessibility provider must not hold up delivery. The
/// worker can only return a check result; it never gets a chance to paste.
#[cfg(any(target_os = "macos", test))]
fn bounded<T: Send + 'static>(limit: Duration, check: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("paste-security".into())
        .spawn(move || {
            let _ = sender.send(check());
        })
        .ok()?;
    receiver.recv_timeout(limit).ok()
}

/// The app the picker was last opened from.
#[derive(Default)]
pub(crate) struct LastTarget(Mutex<Option<Captured>>);

/// Remember the app in front, before the picker shows. Ibis in front keeps the earlier capture.
pub(crate) fn capture(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let front = macos::frontmost();
    #[cfg(not(target_os = "macos"))]
    let front = None;
    let own = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    if let Some(captured) = capture::captured(SYSTEM, front, own)
        && let Ok(mut last) = app.state::<LastTarget>().0.lock()
    {
        *last = Some(captured);
    }
}

/// The app the picker was last opened from, with the name the picker shows
/// for it: the app's own, else the destination table's, else its ID.
pub(crate) fn last_app(app: &AppHandle) -> Option<(AppId, String)> {
    let captured = app.state::<LastTarget>().0.lock().ok()?.clone()?;
    let id = captured.target.app?;
    let name = captured
        .name
        .or_else(|| lookup(&id).map(|known| known.name.to_owned()))
        .unwrap_or_else(|| id.id.clone());
    Some((id, name))
}

/// What happened to a copied citation, for the notice to word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub(crate) enum Delivered {
    Pasted {
        /// The app's own name; `None` when it has none.
        app: Option<String>,
    },
    Copied {
        /// Why Ibis did not paste, when the person can do something about it
        /// or would expect a paste: a `CopyReason` code such as `terminal`.
        reason: Option<String>,
    },
}

/// The system calls a paste needs, so the decision is testable without them.
pub(crate) trait Desk {
    /// Ibis may post key events (macOS Accessibility).
    fn may_post(&self) -> bool;
    /// Show where to give that permission; at most once per run.
    fn ask_to_post(&self);
    /// The app in front, or `None` once the picker has been shown again.
    fn frontmost(&self) -> Option<Front>;
    fn clipboard(&self) -> Option<String>;
    /// Whether the captured app's focused field is secure, checked at paste
    /// time. `None` means an error or timeout: pasting has not proved safe.
    fn secure_field(&self, pid: i32) -> Option<bool>;
    /// The key that types V with Command on the current layout.
    fn paste_key(&self) -> Option<u16>;
    /// Post ⌘ and `key` down and up to `pid` only; false when nothing was posted.
    fn post(&self, pid: i32, key: u16) -> bool;
    fn wait(&self, time: Duration);
}

/// Paste `text`, just copied, into the captured app, or say why it stays copied.
pub(crate) fn deliver(desk: &impl Desk, captured: Option<&Captured>, text: &str, enabled: bool) -> Delivered {
    let unknown = Captured::unknown(SYSTEM);
    let captured = captured.unwrap_or(&unknown);
    let permission = PastePermission {
        enabled,
        granted: enabled && desk.may_post(),
    };
    let copied = |reason: Option<CopyReason>| Delivered::Copied {
        reason: reason
            .and_then(|reason| serde_json::to_value(reason).ok())
            .and_then(|code| code.as_str().map(str::to_owned)),
    };
    match decide(&captured.target, permission) {
        Decision::Copy(CopyReason::NoPermission) => {
            desk.ask_to_post();
            copied(Some(CopyReason::NoPermission))
        }
        // Nothing the person chose or would expect a paste for.
        Decision::Copy(CopyReason::PasteOff | CopyReason::UnsupportedSystem | CopyReason::NoTarget) => copied(None),
        Decision::Copy(reason) => copied(Some(reason)),
        Decision::Paste => {
            let Some(pid) = captured.pid else {
                return copied(None);
            };
            // The same process and app: a process number alone can be reused.
            let id = captured.target.app.as_ref().map(|app| app.id.as_str());
            let back = |front: Option<Front>| front.is_some_and(|front| front.pid == pid && front.id.as_deref() == id);
            let started = Instant::now();
            let mut checks = 0;
            while !back(desk.frontmost()) {
                if checks >= RETURN_CHECKS || started.elapsed() >= RETURN_LIMIT {
                    return copied(None);
                }
                desk.wait(RETURN_STEP);
                checks += 1;
            }
            desk.wait(SETTLE);
            let key = desk.paste_key();
            // Clipboard providers and layout queries can wait on another
            // app. Finish them before asking whether its field is secure.
            if desk.clipboard().as_deref() != Some(text) || !back(desk.frontmost()) || started.elapsed() > LATE {
                return copied(None);
            }
            match desk.secure_field(pid) {
                Some(false) => {}
                Some(true) => return copied(Some(CopyReason::SecureField)),
                None => return copied(None),
            }
            // Focus may have moved while the security provider answered.
            // Never reopen a foreign clipboard provider after that check.
            if !back(desk.frontmost()) || started.elapsed() > LATE {
                return copied(None);
            }
            match key {
                Some(key) if desk.post(pid, key) => Delivered::Pasted {
                    app: captured.name.clone(),
                },
                _ => copied(None),
            }
        }
    }
}

/// Paste into the captured app when it is safe to; the notice then says what happened.
pub(crate) fn deliver_copy(app: &AppHandle, text: &str) -> Delivered {
    let captured = app.state::<LastTarget>().0.lock().ok().and_then(|last| last.clone());
    #[cfg(target_os = "macos")]
    let desk = macos::Mac::new(app);
    #[cfg(not(target_os = "macos"))]
    let desk = CopyOnly;
    let enabled = ibis_core::settings::load(&app.state::<crate::preferences::SettingsFile>().path)
        .settings
        .paste
        .unwrap_or(true);
    deliver(&desk, captured.as_ref(), text, enabled)
}

/// Windows (until #1133) and Linux capture no target, so they only copy.
#[cfg(not(target_os = "macos"))]
struct CopyOnly;

#[cfg(not(target_os = "macos"))]
impl Desk for CopyOnly {
    fn may_post(&self) -> bool {
        false
    }
    fn ask_to_post(&self) {}
    fn frontmost(&self) -> Option<Front> {
        None
    }
    fn clipboard(&self) -> Option<String> {
        None
    }
    fn secure_field(&self, _: i32) -> Option<bool> {
        None
    }
    fn paste_key(&self) -> Option<u16> {
        None
    }
    fn post(&self, _: i32, _: u16) -> bool {
        false
    }
    fn wait(&self, _: Duration) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::{AppId, Target};
    use std::cell::{Cell, RefCell};

    type SecurityQuery = Box<dyn FnOnce() -> Option<bool> + Send>;

    struct Fake {
        may_post: bool,
        asked: Cell<u32>,
        /// Front-most process after each wait.
        fronts: RefCell<Vec<i32>>,
        clipboard: String,
        password: Cell<Option<bool>>,
        password_query: RefCell<Option<SecurityQuery>>,
        password_checks: RefCell<Vec<i32>>,
        /// A deferred clipboard provider lets focus change before it returns.
        password_after_clipboard: Option<bool>,
        /// Simulate focus moving while the security query waits on the app.
        after_password: Option<i32>,
        posts: RefCell<Vec<(i32, u16)>>,
    }

    impl Fake {
        fn new(clipboard: &str) -> Self {
            Self {
                may_post: true,
                asked: Cell::new(0),
                fronts: RefCell::new(vec![42]),
                clipboard: clipboard.into(),
                password: Cell::new(Some(false)),
                password_query: RefCell::default(),
                password_checks: RefCell::default(),
                password_after_clipboard: None,
                after_password: None,
                posts: RefCell::default(),
            }
        }
    }

    impl Desk for Fake {
        fn may_post(&self) -> bool {
            self.may_post
        }
        fn ask_to_post(&self) {
            self.asked.set(self.asked.get() + 1);
        }
        fn frontmost(&self) -> Option<Front> {
            self.fronts.borrow().first().map(|&pid| Front {
                id: Some("org.example.writer".into()),
                pid,
                name: None,
            })
        }
        fn clipboard(&self) -> Option<String> {
            if let Some(secure) = self.password_after_clipboard {
                self.password.set(Some(secure));
            }
            Some(self.clipboard.clone())
        }
        fn secure_field(&self, pid: i32) -> Option<bool> {
            self.password_checks.borrow_mut().push(pid);
            if let Some(front) = self.after_password {
                self.fronts.replace(vec![front]);
            }
            if let Some(query) = self.password_query.borrow_mut().take() {
                bounded(Duration::from_millis(10), query).flatten()
            } else {
                self.password.get()
            }
        }
        fn paste_key(&self) -> Option<u16> {
            Some(0x09)
        }
        fn post(&self, pid: i32, key: u16) -> bool {
            self.posts.borrow_mut().push((pid, key));
            true
        }
        fn wait(&self, _: Duration) {
            let mut fronts = self.fronts.borrow_mut();
            if fronts.len() > 1 {
                fronts.remove(0);
            }
        }
    }

    fn writer(id: &str) -> Captured {
        Captured {
            target: Target {
                system: SYSTEM,
                app: Some(AppId {
                    system: SYSTEM,
                    id: id.into(),
                }),
                secure_field: false,
                elevated: false,
            },
            pid: Some(42),
            name: Some("Writer".into()),
        }
    }

    fn copied(reason: Option<&str>) -> Delivered {
        Delivered::Copied {
            reason: reason.map(str::to_owned),
        }
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn enter_posts_exactly_one_paste_to_the_captured_app_once_it_is_back_in_front() {
        let desk = Fake::new("[@synthetic]");
        // The picker is still in front for two checks.
        desk.fronts.replace(vec![7, 7, 42]);
        let delivered = deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true);
        assert_eq!(
            delivered,
            Delivered::Pasted {
                app: Some("Writer".into())
            }
        );
        assert_eq!(*desk.posts.borrow(), [(42, 0x09)]);
        assert_eq!(*desk.password_checks.borrow(), [42]);
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn secure_fields_copy_with_the_existing_password_reason() {
        let desk = Fake {
            password: Cell::new(Some(true)),
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true),
            copied(Some("secure-field"))
        );
        assert_eq!(*desk.password_checks.borrow(), [42]);
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn a_failed_security_check_only_copies() {
        let desk = Fake {
            password: Cell::new(None),
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true),
            copied(None)
        );
        assert_eq!(*desk.password_checks.borrow(), [42]);
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn a_security_timeout_copies_and_a_late_safe_result_cannot_paste() {
        let (release, blocked) = std::sync::mpsc::channel();
        let (finished, completion) = std::sync::mpsc::channel();
        let desk = Fake {
            password_query: RefCell::new(Some(Box::new(move || {
                blocked.recv().ok()?;
                let _ = finished.send(());
                Some(false)
            }))),
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true),
            copied(None)
        );
        assert_eq!(*desk.password_checks.borrow(), [42]);
        assert!(desk.posts.borrow().is_empty());
        assert!(release.send(()).is_ok());
        assert!(completion.recv_timeout(Duration::from_secs(1)).is_ok());
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn a_field_becoming_secure_during_a_clipboard_read_refuses_the_paste() {
        let desk = Fake {
            password_after_clipboard: Some(true),
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true),
            copied(Some("secure-field"))
        );
        assert_eq!(*desk.password_checks.borrow(), [42]);
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "Linux never pastes")]
    fn focus_moving_during_the_security_check_cancels_the_paste() {
        let desk = Fake {
            after_password: Some(7),
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true),
            copied(None)
        );
        assert_eq!(*desk.password_checks.borrow(), [42]);
        assert!(desk.posts.borrow().is_empty());
    }

    #[test]
    fn nothing_is_posted_when_the_paste_is_unsafe_or_unproven() {
        let cases: [(Fake, Captured, bool, Delivered); 5] = [
            // The process number now belongs to another app.
            (
                Fake::new("[@synthetic]"),
                writer("org.example.other"),
                true,
                copied(None),
            ),
            // Another app replaced the clipboard.
            (
                Fake::new("other text"),
                writer("org.example.writer"),
                true,
                copied(None),
            ),
            // The switch is off.
            (
                Fake::new("[@synthetic]"),
                writer("org.example.writer"),
                false,
                copied(None),
            ),
            // The captured app never came back.
            (
                Fake {
                    fronts: RefCell::new(vec![7]),
                    ..Fake::new("[@synthetic]")
                },
                writer("org.example.writer"),
                true,
                copied(None),
            ),
            // Nothing was in front.
            (Fake::new("[@synthetic]"), Captured::unknown(SYSTEM), true, copied(None)),
        ];
        for (desk, captured, enabled, expected) in cases {
            assert_eq!(deliver(&desk, Some(&captured), "[@synthetic]", enabled), expected);
            assert!(desk.posts.borrow().is_empty());
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn terminals_and_missing_permission_copy_with_a_reason_and_ask_once_per_copy() {
        let desk = Fake::new("[@synthetic]");
        assert_eq!(
            deliver(&desk, Some(&writer("com.apple.Terminal")), "[@synthetic]", true),
            copied(Some("terminal"))
        );
        let desk = Fake {
            may_post: false,
            ..Fake::new("[@synthetic]")
        };
        assert_eq!(
            deliver(&desk, Some(&writer("org.example.writer")), "[@synthetic]", true),
            copied(Some("no-permission"))
        );
        assert_eq!(desk.asked.get(), 1);
        assert!(desk.posts.borrow().is_empty());
    }
}
