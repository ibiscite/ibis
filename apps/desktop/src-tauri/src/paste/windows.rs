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

fn foreground_pid() -> Option<i32> {
    let window = w::HWND::GetForegroundWindow()?;
    i32::try_from(window.GetWindowThreadProcessId().1).ok()
}

pub(super) struct Win {
    app: AppHandle,
    /// The picker showing that this copy belongs to; showing it again stops the paste.
    generation: u64,
}

impl Win {
    /// The picker has just hidden. Windows hands the keyboard to whichever
    /// window is next, so Ibis gives it back to the captured window, if that
    /// window still belongs to the captured process.
    pub(super) fn new(app: &AppHandle) -> Self {
        if let Ok(kept) = WINDOW.lock()
            && let Some((window, pid)) = kept.as_ref()
            && window.GetWindowThreadProcessId().1 == *pid
            && w::HWND::GetForegroundWindow().as_ref() != Some(window)
        {
            window.SetForegroundWindow();
        }
        Self {
            app: app.clone(),
            generation: app.state::<crate::picker::PickerGeneration>().current(),
        }
    }

    fn picker_unchanged(&self) -> bool {
        self.app.state::<crate::picker::PickerGeneration>().current() == self.generation
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

    fn frontmost(&self) -> Option<Front> {
        if !self.picker_unchanged() {
            return None;
        }
        front(&w::HWND::GetForegroundWindow()?, false)
    }

    fn clipboard(&self) -> Option<String> {
        self.app.clipboard().read_text().ok()
    }

    fn secure_field(&self) -> bool {
        let (sender, receiver) = mpsc::channel();
        // A thread of its own: UI Automation needs COM set up on the thread
        // that calls it, and a hung app must not hold the paste.
        std::thread::spawn(move || {
            let password = uiautomation::UIAutomation::new()
                .and_then(|automation| automation.get_focused_element())
                .and_then(|field| field.is_password());
            let _ = sender.send(password.unwrap_or(false));
        });
        receiver.recv_timeout(ASK_LIMIT).unwrap_or(false)
    }

    /// Windows' shortcuts follow the V key's virtual key on every layout.
    fn paste_key(&self) -> Option<u16> {
        Some(co::VK::CHAR_V.raw())
    }

    fn post(&self, pid: i32, key: u16) -> bool {
        if !self.picker_unchanged() || key != co::VK::CHAR_V.raw() || foreground_pid() != Some(pid) {
            return false;
        }
        // One call, so no other input lands between the four events.
        let paste = [
            press(co::VK::CONTROL, false),
            press(co::VK::CHAR_V, false),
            press(co::VK::CHAR_V, true),
            press(co::VK::CONTROL, true),
        ];
        matches!(w::SendInput(&paste), Ok(4))
    }

    fn wait(&self, time: Duration) {
        std::thread::sleep(time);
    }
}
