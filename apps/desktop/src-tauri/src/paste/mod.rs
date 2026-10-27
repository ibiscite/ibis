//! Enter pastes into the app the picker was opened from (#27, #1241).
//!
//! After a clean copy has hidden the picker, Ibis posts one ⌘V, only to the
//! captured process, once that app is in front again and the clipboard still
//! holds the citation. `ibis_core::destination::decide` says when it must only
//! copy, and the notice says why. The citation stays on the clipboard either
//! way (ADR 0009).

use std::sync::Mutex;
use std::time::Duration;

use ibis_core::destination::{CopyReason, Decision, PastePermission, System, decide};
use serde::Serialize;
use tauri::{AppHandle, Manager};

mod capture;
#[cfg(target_os = "macos")]
mod macos;

use capture::Captured;

const SYSTEM: System = if cfg!(target_os = "macos") {
    System::Macos
} else if cfg!(target_os = "windows") {
    System::Windows
} else {
    System::Linux
};

/// How long the captured app gets to come back to the front after the picker hides.
const RETURN_LIMIT: Duration = Duration::from_millis(1_000);
const RETURN_STEP: Duration = Duration::from_millis(25);

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
    /// Ask for that permission; the system prompts only the first time.
    fn ask_to_post(&self);
    fn frontmost(&self) -> Option<i32>;
    fn clipboard(&self) -> Option<String>;
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
            // Another app may have taken the clipboard since the copy.
            if desk.clipboard().as_deref() != Some(text) {
                return copied(None);
            }
            let mut waited = Duration::ZERO;
            while desk.frontmost() != Some(pid) {
                if waited >= RETURN_LIMIT {
                    return copied(None);
                }
                desk.wait(RETURN_STEP);
                waited += RETURN_STEP;
            }
            match desk.paste_key() {
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
    let desk = macos::Mac(app.clone());
    #[cfg(not(target_os = "macos"))]
    let desk = CopyOnly;
    deliver(&desk, captured.as_ref(), text, true)
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
    fn frontmost(&self) -> Option<i32> {
        None
    }
    fn clipboard(&self) -> Option<String> {
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

    struct Fake {
        may_post: bool,
        asked: Cell<u32>,
        /// Front-most process after each wait.
        fronts: RefCell<Vec<i32>>,
        clipboard: String,
        posts: RefCell<Vec<(i32, u16)>>,
    }

    impl Fake {
        fn new(clipboard: &str) -> Self {
            Self {
                may_post: true,
                asked: Cell::new(0),
                fronts: RefCell::new(vec![42]),
                clipboard: clipboard.into(),
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
        fn frontmost(&self) -> Option<i32> {
            self.fronts.borrow().first().copied()
        }
        fn clipboard(&self) -> Option<String> {
            Some(self.clipboard.clone())
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
    }

    #[test]
    fn nothing_is_posted_when_the_paste_is_unsafe_or_unproven() {
        let cases: [(Fake, Captured, bool, Delivered); 4] = [
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
