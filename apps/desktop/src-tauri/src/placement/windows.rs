use tauri::Monitor;
use winsafe::{HMONITOR, HWND, POINT, co};

use super::Area;

pub(super) fn focused_window() -> Option<Area> {
    let window = HWND::GetForegroundWindow()?;
    // MonitorFromWindow uses Windows' largest-intersection policy, including
    // minimized windows' restored bounds, in physical per-monitor DPI space.
    let monitor = window
        .MonitorFromWindow(co::MONITOR::DEFAULTTONEAREST)
        .GetMonitorInfo()
        .ok()?;
    let bounds = monitor.rcMonitor;
    Some(Area {
        x: f64::from(bounds.left),
        y: f64::from(bounds.top),
        width: f64::from(bounds.right - bounds.left),
        height: f64::from(bounds.bottom - bounds.top),
    })
}

pub(super) fn display_key(monitor: &Monitor) -> Option<String> {
    let at = monitor.position();
    let name = HMONITOR::MonitorFromPoint(POINT::with(at.x, at.y), co::MONITOR::DEFAULTTONULL)
        .GetMonitorInfo()
        .ok()?
        .szDevice();
    // The adapter's \\.\DISPLAYn name is transient; its active monitor's device
    // interface path is stable across desktop rearrangements and DPI changes.
    for device in winsafe::EnumDisplayDevices(Some(&name), Some(co::EDD::GET_DEVICE_INTERFACE_NAME)) {
        let device = device.ok()?;
        if device.StateFlags.has(co::DISPLAY_DEVICE::ATTACHED_TO_DESKTOP) {
            let path = device.DeviceID();
            if !path.is_empty() {
                return Some(format!("windows:{path}"));
            }
        }
    }
    None
}

pub(super) fn usable(_monitor: &Monitor) -> Option<Area> {
    None // Tauri GetMonitorInfo's rcWork includes taskbars on every edge.
}
