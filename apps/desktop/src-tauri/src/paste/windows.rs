//! The Windows calls behind a paste (#1133), through the safe winsafe and
//! uiautomation bindings only.

use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use winsafe::{self as w, co};

use super::capture::Front;
use super::{PasswordQueries, PasteEvent, PasteKeys, QueryWorker, TicketDesk};

static NAME_WORKER: LazyLock<QueryWorker> = LazyLock::new(QueryWorker::default);
static PASSWORD_QUERIES: LazyLock<PasswordQueries> = LazyLock::new(PasswordQueries::default);
static KEYS: Mutex<PasteKeys> = Mutex::new(PasteKeys {
    control: false,
    v: false,
});

/// UI Automation waits on the app in front, which may be busy. A field that
/// does not answer in time has not proved that it is safe to paste.
const ASK_LIMIT: Duration = Duration::from_millis(250);

/// winsafe handles are Send but neither Clone nor Sync. Share an immutable
/// HWND by borrowing it under a lock; no raw copying or unsafe is needed.
#[derive(Clone)]
pub(crate) struct Window(Arc<Mutex<w::HWND>>);

/// The app in front, before the picker shows; its window is kept unless it is
/// Ibis itself, so showing the picker again keeps the earlier one.
pub(super) fn capture() -> (Option<Front>, Option<Window>) {
    let window = w::HWND::GetForegroundWindow();
    let front = window.as_ref().and_then(|window| front(window, true));
    (front, window.map(|window| Window(Arc::new(Mutex::new(window)))))
}

/// The app behind `window`, by executable file name and process; never the
/// window's title or the executable's folder. Its name and rights are read
/// only for a capture, not for each check while the paste waits.
fn front(window: &w::HWND, capture: bool) -> Option<Front> {
    let (_, pid) = window.GetWindowThreadProcessId();
    let process = w::HPROCESS::OpenProcess(co::PROCESS::QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let path = process.QueryFullProcessImageName(co::PROCESS_NAME::WIN32).ok()?;
    let id = path.rsplit(['\\', '/']).next().map(str::to_owned);
    let name = if capture {
        let executable = id.clone().unwrap_or_default();
        // Version resources can live on an unavailable share. Only the
        // worker touches that file; the picker waits at most 50 ms.
        Some(super::display_name(&NAME_WORKER, executable, move || described(&path)))
    } else {
        None
    };
    Some(Front {
        id,
        pid: i32::try_from(pid).ok()?,
        name,
        elevated: capture && above_ibis(&process),
    })
}

/// The app's own name from its version resource, such as "Microsoft Word".
fn described(path: &str) -> Option<String> {
    let info = w::HVERSIONINFO::GetFileVersionInfo(path).ok()?;
    let language = *info.langs_and_cps().ok()?.first()?;
    let name = info.str_val(language, "FileDescription").ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// The process runs at a higher integrity level than Ibis, so Windows would
/// silently drop keystrokes sent to it. A level Ibis cannot read counts as higher.
fn above_ibis(process: &w::HPROCESS) -> bool {
    match (integrity(process), integrity(&w::HPROCESS::GetCurrentProcess())) {
        (Some(target), Some(own)) => target > own,
        _ => true,
    }
}

/// The mandatory label's level, such as 0x2000 for medium and 0x3000 for high.
fn integrity(process: &w::HPROCESS) -> Option<u32> {
    let token = process.OpenProcessToken(co::TOKEN::QUERY).ok()?;
    let info = token
        .GetTokenInformation(co::TOKEN_INFORMATION_CLASS::IntegrityLevel)
        .ok()?;
    let w::TokenInfo::IntegrityLevel(label) = info else {
        return None;
    };
    label.Label.Sid()?.SubAuthority().last().map(co::RID::raw)
}

/// Keys the person may still hold from the picker. `SendInput` would add them
/// to Ctrl+V, and Ctrl+Alt+V types a character on some layouts.
const HELD: [co::VK; 6] = [
    co::VK::SHIFT,
    co::VK::CONTROL,
    co::VK::MENU,
    co::VK::LWIN,
    co::VK::RWIN,
    co::VK::RETURN,
];

fn keys_held() -> bool {
    HELD.into_iter().any(w::GetAsyncKeyState)
}

fn foreground_pid() -> Option<i32> {
    let window = w::HWND::GetForegroundWindow()?;
    i32::try_from(window.GetWindowThreadProcessId().1).ok()
}

/// The focused field of the app in front takes a password.
fn password_focused(pid: i32) -> Option<bool> {
    let native_pid = u32::try_from(pid).ok()?;
    // A thread of its own: UI Automation needs COM set up on the thread that
    // calls it, and a hung app must not hold the paste.
    // uiautomation 0.25.1 exposes neither CUIAutomation8 nor connection or
    // transaction timeout setters. Keep the caller deadline and PID permits.
    PASSWORD_QUERIES
        .run(pid, ASK_LIMIT, move || {
            if foreground_pid() != Some(pid) {
                return None;
            }
            let field = uiautomation::UIAutomation::new().ok()?.get_focused_element().ok()?;
            if field.get_process_id().ok()? != native_pid {
                return None;
            }
            field.is_password().ok()
        })
        .flatten()
}

/// Ctrl+V to whatever has the keyboard, in one call, so no other input lands
/// between the four events.
fn send_paste(ready: impl Fn() -> bool) -> bool {
    let Ok(mut keys) = KEYS.lock() else {
        warn_key_release();
        return false;
    };
    keys.paste(send_inputs, std::thread::sleep, warn_key_release, ready)
}

fn send_inputs(events: &[PasteEvent]) -> u32 {
    let inputs: Vec<_> = events
        .iter()
        .map(|event| match event {
            PasteEvent::ControlDown => press(co::VK::CONTROL, false),
            PasteEvent::VDown => press(co::VK::CHAR_V, false),
            PasteEvent::VUp => press(co::VK::CHAR_V, true),
            PasteEvent::ControlUp => press(co::VK::CONTROL, true),
        })
        .collect();
    w::SendInput(&inputs).unwrap_or(0)
}

fn warn_key_release() {
    crate::diagnostics::report(Err::<(), ()>(()), crate::diagnostics::NativeIssue::PasteKeyRelease);
}

pub(super) struct Win {
    app: AppHandle,
}

impl Win {
    /// Construction never reads a capture, generation or clipboard baseline.
    /// The successful copy's ticket owns those throughout delivery.
    pub(super) fn new(app: &AppHandle) -> Self {
        Self { app: app.clone() }
    }
}

impl TicketDesk for Win {
    type Window = Window;

    fn generation(&self) -> u64 {
        self.app.state::<crate::picker::PickerGeneration>().current()
    }

    fn picker_hidden(&self) -> bool {
        self.app
            .get_webview_window("main")
            .is_some_and(|picker| picker.is_visible().ok() == Some(false))
    }

    fn restore(&self, window: &Window, pid: i32) -> bool {
        let Ok(window) = window.0.lock() else {
            return false;
        };
        if i32::try_from(window.GetWindowThreadProcessId().1).ok() != Some(pid) {
            return false;
        }
        if w::HWND::GetForegroundWindow().as_ref() != Some(&*window) {
            window.SetForegroundWindow();
        }
        true
    }

    fn window_in_front(&self, window: &Window) -> bool {
        window
            .0
            .lock()
            .is_ok_and(|window| w::HWND::GetForegroundWindow().as_ref() == Some(&*window))
    }

    fn clipboard_sequence(&self) -> Option<u32> {
        clipboard_win::seq_num().map(std::num::NonZeroU32::get)
    }

    fn post_if(&self, pid: i32, key: u16, ready: impl Fn() -> bool) -> bool {
        send_paste(|| ready() && key == co::VK::CHAR_V.raw() && !keys_held() && foreground_pid() == Some(pid))
    }
    fn release_keys(&self) -> bool {
        let Ok(mut keys) = KEYS.lock() else {
            warn_key_release();
            return false;
        };
        keys.release(send_inputs, std::thread::sleep, warn_key_release)
    }

    fn keys_unreleased(&self) -> bool {
        KEYS.lock().map_or(true, |keys| keys.pending())
    }

    /// Windows asks no permission to send keys; elevated apps are refused by the capture.
    fn may_post(&self) -> bool {
        true
    }

    fn ask_to_post(&self) {}

    /// `None` also while another window, even of the same app, is in front,
    /// and while a key from the picker is still held, so the paste waits for
    /// the keyboard to be free as it waits for the window.
    fn frontmost(&self) -> Option<Front> {
        if keys_held() {
            return None;
        }
        front(&w::HWND::GetForegroundWindow()?, false)
    }

    fn secure_field(&self, pid: i32) -> Option<bool> {
        password_focused(pid)
    }

    /// Windows' shortcuts follow the V key's virtual key on every layout.
    fn paste_key(&self) -> Option<u16> {
        Some(co::VK::CHAR_V.raw())
    }

    fn wait(&self, time: Duration) {
        std::thread::sleep(time);
    }
}

/// One key going down or up.
fn press(key: co::VK, up: bool) -> w::HwKbMouse {
    w::HwKbMouse::Kb(w::KEYBDINPUT {
        wVk: key,
        wScan: u16::try_from(w::MapVirtualKey(u32::from(key.raw()), co::MAPVK::VK_TO_VSC)).unwrap_or(0),
        dwFlags: if up {
            co::KEYEVENTF::KEYUP
        } else {
            co::KEYEVENTF::NoValue
        },
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paste::notepad_test::{Session, activate, wait_for};
    use std::process::Command;

    /// Never log titles or full executable paths, including on a developer's desktop.
    fn describe_window(window: Option<&w::HWND>) -> String {
        let Some(window) = window else {
            return "no window".to_owned();
        };
        let (thread, pid) = window.GetWindowThreadProcessId();
        format!(
            "HWND={window}, PID={pid}, thread={thread}, class={:?}, executable={:?}",
            window.GetClassName(),
            front(window, false).and_then(|front| front.id)
        )
    }

    /// A packaged Notepad can hand off to an existing process. Match its class
    /// or executable, then the synthetic document, never the launcher's PID.
    fn notepad_window(document: &str) -> Option<w::HWND> {
        let mut found = None;
        w::EnumWindows(|window| {
            if found.is_none()
                && window.IsWindowVisible()
                && (window
                    .GetClassName()
                    .is_ok_and(|class| class.eq_ignore_ascii_case("Notepad"))
                    || front(&window, false)
                        .and_then(|front| front.id)
                        .is_some_and(|id| id.eq_ignore_ascii_case("notepad.exe")))
                && window.GetWindowText().is_ok_and(|title| title.contains(document))
            {
                found = Some(window);
            }
            // Stopping enumeration early makes winsafe return GetLastError,
            // which need not describe a failure. Finish the enumeration instead.
            true
        })
        .unwrap_or_else(|error| panic!("enumerating Notepad windows: {error}"));
        found
    }

    /// The caller deadline also bounds a busy Notepad UI Automation provider.
    fn focused_text(worker: &QueryWorker, pid: u32) -> Option<String> {
        worker
            .run(Duration::from_secs(1), move || {
                let field = uiautomation::UIAutomation::new().ok()?.get_focused_element().ok()?;
                if field.get_process_id().ok()? != pid {
                    return None;
                }
                field
                    .get_pattern::<uiautomation::patterns::UITextPattern>()
                    .ok()?
                    .get_document_range()
                    .ok()?
                    .get_text(-1)
                    .ok()
            })
            .flatten()
    }

    fn tap(key: co::VK) {
        let sent = w::SendInput(&[press(key, false), press(key, true)]);
        if sent != Ok(2) {
            let released = w::SendInput(&[press(key, true)]);
            panic!("foreground key tap {key:?}: {sent:?}; key release: {released:?}");
        }
    }

    /// Recover the editor explicitly: ALT can arrive after asynchronous
    /// activation and select Notepad's menu instead of its document.
    fn focus_document(worker: &QueryWorker, hwnd: isize, pid: u32) -> Option<()> {
        worker
            .run(Duration::from_secs(1), move || {
                let automation = uiautomation::UIAutomation::new().ok()?;
                let root = automation.element_from_handle(hwnd.into()).ok()?;
                let editor = automation
                    .create_matcher()
                    .from(root)
                    .depth(20)
                    .timeout(0)
                    .filter_fn(Box::new(|field: &uiautomation::UIElement| {
                        // Modern and classic Notepad editors; exclude search
                        // boxes and tab controls in a restored session.
                        Ok(matches!(field.get_classname()?.as_str(), "RichEditD2DPT" | "Edit"))
                    }))
                    .find_first()
                    .ok()?;
                if editor.get_process_id().ok()? != pid {
                    return None;
                }
                editor.set_focus().ok()?;
                let focused = automation.get_focused_element().ok()?;
                automation.compare_elements(&editor, &focused).ok()?.then_some(())
            })
            .flatten()
    }

    /// Neither a launcher nor a handed-off process proves exclusive ownership:
    /// both can contain restored documents. Keep handles, never force-kill.
    struct Notepad {
        session: Session<w::guard::CloseHandleGuard<w::HPROCESS>>,
        window: Option<w::HWND>,
    }

    impl Drop for Notepad {
        fn drop(&mut self) {
            if std::thread::panicking() {
                eprintln!("Notepad target: {}", describe_window(self.window.as_ref()));
                eprintln!(
                    "In front on failure: {}",
                    describe_window(w::HWND::GetForegroundWindow().as_ref())
                );
            }
            eprintln!("Leaving Notepad open: its process may contain unrelated restored or shared documents.");
        }
    }

    #[test]
    fn ibis_reads_its_own_integrity_level_and_is_not_above_itself() {
        let own = w::HPROCESS::GetCurrentProcess();
        assert!(integrity(&own).is_some_and(|level| level >= 0x2000));
        assert!(!above_ibis(&own));
    }

    /// The whole Windows path on a real desktop: capture the app in front, see
    /// it is no password field, send one Ctrl+V and read the text it received.
    #[test]
    #[ignore = "drives Notepad on a desktop session; CI's Windows job runs it"]
    fn one_ctrl_v_pastes_the_clipboard_into_the_app_in_front() {
        let text = "[@synthetic2026]";
        let set = Command::new("powershell")
            .args(["-NoProfile", "-Command", &format!("Set-Clipboard -Value '{text}'")])
            .status();
        assert!(set.is_ok_and(|status| status.success()));
        // Open a new empty file so restored tabs cannot satisfy the readback.
        // Close our file handle before Notepad opens it on Windows.
        let file = tempfile::Builder::new()
            .prefix("ibis-notepad-")
            .suffix(".txt")
            .tempfile()
            .unwrap_or_else(|error| panic!("creating the synthetic document: {error}"))
            .into_temp_path();
        let document = file
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("synthetic document has no UTF-8 name"));
        let mut notepad = Notepad {
            session: Session {
                launcher: Command::new("notepad.exe")
                    .arg(&file)
                    .spawn()
                    .unwrap_or_else(|error| panic!("notepad: {error}")),
                process: None,
            },
            window: None,
        };
        notepad.window = wait_for(Duration::from_secs(20), || notepad_window(document));
        let window = notepad
            .window
            .as_ref()
            .unwrap_or_else(|| panic!("Notepad never opened the synthetic document"));
        let front = front(window, true).unwrap_or_else(|| panic!("cannot inspect Notepad's process"));
        assert!(!front.elevated);
        let pid = window.GetWindowThreadProcessId().1;
        notepad.session.process = Some(
            w::HPROCESS::OpenProcess(
                co::PROCESS::QUERY_LIMITED_INFORMATION | co::PROCESS::SYNCHRONIZE,
                false,
                pid,
            )
            .unwrap_or_else(|error| panic!("retaining Notepad's process: {error}")),
        );
        let process = notepad
            .session
            .process
            .as_ref()
            .unwrap_or_else(|| panic!("Notepad process handle"));
        window
            .ShowWindowAsync(co::SW::RESTORE)
            .unwrap_or_else(|error| panic!("restoring Notepad: {error}"));
        let in_front = || {
            process.WaitForSingleObject(Some(0)) == Ok(co::WAIT::TIMEOUT)
                && window.GetWindowThreadProcessId().1 == pid
                && window.GetWindowText().is_ok_and(|title| title.contains(document))
                && w::HWND::GetForegroundWindow().as_ref() == Some(window)
        };
        let worker = QueryWorker::default();
        let hwnd = window.ptr() as isize;
        if !activate(
            Duration::from_secs(5),
            || {
                // SetForegroundWindow can return before the target's input
                // queue activates it. Wait before considering the ALT fallback.
                window.SetForegroundWindow();
            },
            in_front,
            || tap(co::VK::MENU),
            |used_alt| {
                assert!(in_front(), "Notepad lost the foreground before focus recovery");
                if used_alt {
                    // Cancel menu mode even if ALT reached the newly activated
                    // window. Then focus the actual editor, not a search box.
                    tap(co::VK::ESCAPE);
                }
                assert!(
                    wait_for(Duration::from_secs(5), || {
                        if !in_front() {
                            return None;
                        }
                        focus_document(&worker, hwnd, pid)
                    })
                    .is_some(),
                    "Notepad's document never received keyboard focus"
                );
            },
        ) {
            // A timeout alone is not grounds for skipping. Verify a live,
            // usable target and an explicit OS denial of our foreground right.
            assert_eq!(
                process.WaitForSingleObject(Some(0)),
                Ok(co::WAIT::TIMEOUT),
                "Notepad exited during activation"
            );
            assert_eq!(
                window.GetWindowThreadProcessId().1,
                pid,
                "Notepad's window changed process"
            );
            assert!(window.IsWindow() && window.IsWindowVisible() && window.IsWindowEnabled());
            let permission = w::AllowSetForegroundWindow(Some(w::GetCurrentProcessId()));
            let foreground = describe_window(w::HWND::GetForegroundWindow().as_ref());
            if permission == Err(co::ERROR::ACCESS_DENIED) {
                eprintln!(
                    "SKIP one_ctrl_v_pastes_the_clipboard_into_the_app_in_front: Windows denied the test process foreground permission (AllowSetForegroundWindow: ACCESS_DENIED) after the ALT tap and activation deadline; target: {}; in front: {foreground}",
                    describe_window(Some(window))
                );
                return;
            }
            panic!("Notepad never came to the front; foreground permission: {permission:?}; in front: {foreground}");
        }
        assert!(
            wait_for(Duration::from_secs(5), || {
                (in_front() && focused_text(&worker, pid)?.trim().is_empty()).then_some(())
            })
            .is_some(),
            "Notepad's focused document never became empty and readable"
        );
        assert!(
            wait_for(Duration::from_secs(5), || (in_front()
                && password_focused(front.pid) == Some(false))
            .then_some(()))
            .is_some(),
            "Notepad's focused field was not confirmed to be a non-password field"
        );
        assert!(!keys_held());
        assert!(send_paste(|| in_front()
            && !keys_held()
            && foreground_pid() == Some(front.pid)));
        let pasted = wait_for(Duration::from_secs(5), || {
            let value = focused_text(&worker, pid)?;
            (in_front() && value.trim() == text).then_some(value)
        });
        assert!(
            pasted.is_some(),
            "one Ctrl+V did not produce exactly the citation in Notepad's focused document"
        );
    }
}
