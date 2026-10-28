//! The Windows calls behind a paste (#1133), through the safe winsafe and
//! uiautomation bindings only.

use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use winsafe::{self as w, co};

use super::capture::Front;
use super::{Desk, PasswordQueries, PasteEvent, PasteKeys, QueryWorker, TicketDesk};

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

impl Desk for Win {
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

    /// Windows must go through the shared ticket guard's sequence proof.
    fn clipboard_matches(&self, _: &str) -> bool {
        false
    }

    fn secure_field(&self, pid: i32) -> Option<bool> {
        password_focused(pid)
    }

    /// Windows' shortcuts follow the V key's virtual key on every layout.
    fn paste_key(&self) -> Option<u16> {
        Some(co::VK::CHAR_V.raw())
    }

    fn post(&self, pid: i32, key: u16) -> bool {
        self.post_if(pid, key, || true)
    }

    fn wait(&self, time: Duration) {
        std::thread::sleep(time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::Instant;

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
        let mut notepad = Command::new("notepad.exe")
            .spawn()
            .unwrap_or_else(|e| panic!("notepad: {e}"));
        let started = Instant::now();
        let front = loop {
            let front = w::HWND::GetForegroundWindow().and_then(|window| front(&window, true));
            if let Some(front) = front.filter(|front| {
                front
                    .id
                    .as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case("notepad.exe"))
            }) {
                break front;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "Notepad never came to the front"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        assert!(!front.elevated);
        assert_eq!(password_focused(front.pid), Some(false));
        assert!(!keys_held());
        assert!(send_paste(|| true));
        let started = Instant::now();
        let pasted = loop {
            let value = uiautomation::UIAutomation::new()
                .and_then(|automation| automation.get_focused_element())
                .and_then(|field| field.get_pattern::<uiautomation::patterns::UITextPattern>())
                .and_then(|text| text.get_document_range())
                .and_then(|document| document.get_text(-1))
                .unwrap_or_default();
            if value.contains(text) || started.elapsed() > Duration::from_secs(5) {
                break value;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        // Notepad may be a launcher for another process: close the one in front.
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &front.pid.to_string()])
            .status();
        let _ = notepad.kill();
        let _ = notepad.wait();
        assert_eq!(pasted.trim(), text);
    }
}
