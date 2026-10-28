//! The Windows calls behind a paste (#1133), through the safe winsafe and
//! uiautomation bindings only.

use std::sync::{Mutex, mpsc};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use winsafe::{self as w, co, prelude::*};

use super::Desk;
use super::capture::Front;

/// The window the picker was opened from and its process, to give it the
/// keyboard back once the picker hides.
static WINDOW: Mutex<Option<(w::HWND, u32)>> = Mutex::new(None);

/// UI Automation waits on the app in front, which may be busy. A field that
/// does not answer in time is treated as an ordinary field: a citation in a
/// password box is harmless, a paste that never happens is not.
const ASK_LIMIT: Duration = Duration::from_millis(250);

/// The app in front, before the picker shows; its window is kept unless it is
/// Ibis itself, so showing the picker again keeps the earlier one.
pub(super) fn capture(own: i32) -> Option<Front> {
    let window = w::HWND::GetForegroundWindow();
    let front = window.as_ref().and_then(|window| front(window, true));
    if let Ok(mut kept) = WINDOW.lock() {
        match (window, &front) {
            (Some(window), Some(front)) if front.pid != own => {
                *kept = u32::try_from(front.pid).ok().map(|pid| (window, pid));
            }
            (_, Some(_)) => {}
            _ => *kept = None,
        }
    }
    front
}

/// The app behind `window`, by executable file name and process; never the
/// window's title or the executable's folder. Its name and rights are read
/// only for a capture, not for each check while the paste waits.
fn front(window: &w::HWND, capture: bool) -> Option<Front> {
    let (_, pid) = window.GetWindowThreadProcessId();
    let process = w::HPROCESS::OpenProcess(co::PROCESS::QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let path = process.QueryFullProcessImageName(co::PROCESS_NAME::WIN32).ok()?;
    Some(Front {
        id: path.rsplit(['\\', '/']).next().map(str::to_owned),
        pid: i32::try_from(pid).ok()?,
        name: if capture { described(&path) } else { None },
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
fn password_focused() -> bool {
    let (sender, receiver) = mpsc::channel();
    // A thread of its own: UI Automation needs COM set up on the thread that
    // calls it, and a hung app must not hold the paste.
    std::thread::spawn(move || {
        let password = uiautomation::UIAutomation::new()
            .and_then(|automation| automation.get_focused_element())
            .and_then(|field| field.is_password());
        let _ = sender.send(password.unwrap_or(false));
    });
    receiver.recv_timeout(ASK_LIMIT).unwrap_or(false)
}

/// Ctrl+V to whatever has the keyboard, in one call, so no other input lands
/// between the four events.
fn send_paste() -> bool {
    let paste = [
        press(co::VK::CONTROL, false),
        press(co::VK::CHAR_V, false),
        press(co::VK::CHAR_V, true),
        press(co::VK::CONTROL, true),
    ];
    matches!(w::SendInput(&paste), Ok(4))
}

pub(super) struct Win {
    app: AppHandle,
    /// The picker showing that this copy belongs to; showing it again stops the paste.
    generation: u64,
    /// The captured window, so a paste never lands in another window of the
    /// same app; `None` when there is nothing safe to paste into.
    window: Option<w::HWND>,
    /// The clipboard's change count after the copy. Another app's clipboard
    /// is never read: a hung owner of delayed text would block the read.
    clipboard: u32,
}

impl Win {
    /// The picker has just hidden. Windows hands the keyboard to whichever
    /// window is next, so Ibis gives it back to the captured window, if that
    /// window still belongs to the captured process. A picker that is showing
    /// again was opened from somewhere new: this copy then only stays copied,
    /// and focus is left alone.
    pub(super) fn new(app: &AppHandle) -> Self {
        let reopened = app
            .get_webview_window("main")
            .is_none_or(|picker| picker.is_visible().unwrap_or(true));
        // Taken, not copied: each showing of the picker captures afresh.
        let window = WINDOW
            .lock()
            .ok()
            .filter(|_| !reopened)
            .and_then(|mut kept| kept.take())
            .filter(|(window, pid)| window.GetWindowThreadProcessId().1 == *pid)
            .map(|(window, _)| window);
        if let Some(window) = &window
            && w::HWND::GetForegroundWindow().as_ref() != Some(window)
        {
            window.SetForegroundWindow();
        }
        Self {
            app: app.clone(),
            generation: app.state::<crate::picker::PickerGeneration>().current(),
            window,
            clipboard: w::HCLIPBOARD::NULL.GetClipboardSequenceNumber(),
        }
    }

    fn picker_unchanged(&self) -> bool {
        self.app.state::<crate::picker::PickerGeneration>().current() == self.generation
    }

    /// The captured window itself is in front, not just its app.
    fn window_in_front(&self) -> bool {
        self.window.is_some() && w::HWND::GetForegroundWindow() == self.window
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
    /// Windows asks no permission to send keys; elevated apps are refused by the capture.
    fn may_post(&self) -> bool {
        true
    }

    fn ask_to_post(&self) {}

    /// `None` also while another window, even of the same app, is in front,
    /// and while a key from the picker is still held, so the paste waits for
    /// the keyboard to be free as it waits for the window.
    fn frontmost(&self) -> Option<Front> {
        if !self.picker_unchanged() || !self.window_in_front() || keys_held() {
            return None;
        }
        front(self.window.as_ref()?, false)
    }

    /// The citation Ibis wrote, while nothing has replaced it since.
    fn clipboard(&self) -> Option<String> {
        if w::HCLIPBOARD::NULL.GetClipboardSequenceNumber() != self.clipboard {
            return None;
        }
        self.app.clipboard().read_text().ok()
    }

    fn secure_field(&self) -> bool {
        password_focused()
    }

    /// Windows' shortcuts follow the V key's virtual key on every layout.
    fn paste_key(&self) -> Option<u16> {
        Some(co::VK::CHAR_V.raw())
    }

    fn post(&self, pid: i32, key: u16) -> bool {
        if !self.picker_unchanged()
            || key != co::VK::CHAR_V.raw()
            || keys_held()
            || !self.window_in_front()
            || foreground_pid() != Some(pid)
        {
            return false;
        }
        send_paste()
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
        assert!(!password_focused());
        assert!(!keys_held());
        assert!(send_paste());
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
