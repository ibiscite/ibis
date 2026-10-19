use super::policy::{Appearance, Preferences};
use tauri::{AppHandle, WebviewWindow};

pub(super) fn material() -> Appearance {
    #[cfg(target_os = "macos")]
    return Appearance::Vibrancy;
    #[cfg(target_os = "windows")]
    return Appearance::Mica;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Appearance::Opaque
}

pub(super) fn apply(window: &WebviewWindow) -> Result<(), ()> {
    #[cfg(target_os = "macos")]
    return window_vibrancy::apply_vibrancy(
        window,
        window_vibrancy::NSVisualEffectMaterial::Popover,
        Some(window_vibrancy::NSVisualEffectState::Active),
        None,
    )
    .map_err(|_| ());
    #[cfg(target_os = "windows")]
    return window_vibrancy::apply_mica(window, None).map_err(|_| ());
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = window;
        Err(())
    }
}

pub(super) fn clear(window: &WebviewWindow) -> Result<(), ()> {
    #[cfg(target_os = "macos")]
    return window_vibrancy::clear_vibrancy(window).map(|_| ()).map_err(|_| ());
    #[cfg(target_os = "windows")]
    return window_vibrancy::clear_mica(window).map_err(|_| ());
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = window;
        Ok(())
    }
}

pub(super) fn preferences(app: &AppHandle) -> Option<Preferences> {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        if !app.state::<WatchAvailable>().0 {
            return None;
        }
        let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
        Some(Preferences {
            high_contrast: workspace.accessibilityDisplayShouldIncreaseContrast(),
            reduce_transparency: workspace.accessibilityDisplayShouldReduceTransparency(),
        })
    }
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager;
        let state = app.state::<std::sync::Mutex<Option<WindowsPreferences>>>();
        let (contrast, effects) = {
            let observers = state.lock().ok()?;
            let observers = observers.as_ref()?;
            (observers.contrast.clone(), observers.effects.clone())
        };
        Some(Preferences {
            high_contrast: contrast.HighContrast().ok()?,
            reduce_transparency: !effects.AdvancedEffectsEnabled().ok()?,
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        None
    }
}

pub(super) fn watch(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager;
        // Register the empty state first: a preference callback can arrive
        // during observer setup and must see an opaque fallback, not panic.
        app.manage(std::sync::Mutex::new(None::<WindowsPreferences>));
        let observers = windows_watch(app);
        if let Ok(mut state) = app.state::<std::sync::Mutex<Option<WindowsPreferences>>>().lock() {
            *state = observers;
        } else {
            super::warning();
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use tauri::Manager;
        // objc2's notification observer registration requires unsafe. Poll only
        // while visible, at most one pending main-thread refresh, instead.
        let handle = app.clone();
        let pending = Arc::new(AtomicBool::new(false));
        let watcher = std::thread::Builder::new()
            .name("native-appearance".to_owned())
            .spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let visible = handle
                        .get_webview_window("main")
                        .and_then(|window| window.is_visible().ok())
                        .unwrap_or(false);
                    if !visible || pending.swap(true, Ordering::Relaxed) {
                        continue;
                    }
                    let task = handle.clone();
                    let complete = pending.clone();
                    if handle
                        .run_on_main_thread(move || {
                            super::refresh(&task);
                            complete.store(false, Ordering::Relaxed);
                        })
                        .is_err()
                    {
                        pending.store(false, Ordering::Relaxed);
                        super::warning();
                    }
                }
            });
        let available = watcher.is_ok();
        if !available {
            super::warning();
        }
        app.manage(WatchAvailable(available));
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = app;
}

#[cfg(target_os = "macos")]
struct WatchAvailable(bool);

#[cfg(target_os = "windows")]
struct WindowsPreferences {
    contrast: windows::UI::ViewManagement::AccessibilitySettings,
    effects: windows::UI::ViewManagement::UISettings,
    contrast_token: i64,
    effects_token: i64,
}

#[cfg(target_os = "windows")]
impl Drop for WindowsPreferences {
    fn drop(&mut self) {
        let contrast = self.contrast.RemoveHighContrastChanged(self.contrast_token);
        let effects = self.effects.RemoveAdvancedEffectsEnabledChanged(self.effects_token);
        if contrast.is_err() || effects.is_err() {
            super::warning();
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_watch(app: &AppHandle) -> Option<WindowsPreferences> {
    use windows::{
        Foundation::TypedEventHandler,
        UI::ViewManagement::{AccessibilitySettings, UISettings},
    };
    let setup = || -> windows::core::Result<WindowsPreferences> {
        let contrast = AccessibilitySettings::new()?;
        let effects = UISettings::new()?;
        let handle = app.clone();
        let contrast_token = contrast.HighContrastChanged(&TypedEventHandler::<
            AccessibilitySettings,
            windows::core::IInspectable,
        >::new(move |_, _| {
            super::schedule(&handle);
            Ok(())
        }))?;
        let handle = app.clone();
        let effects_token = match effects.AdvancedEffectsEnabledChanged(&TypedEventHandler::<
            UISettings,
            windows::core::IInspectable,
        >::new(move |_, _| {
            super::schedule(&handle);
            Ok(())
        })) {
            Ok(token) => token,
            Err(error) => {
                let _ = contrast.RemoveHighContrastChanged(contrast_token);
                return Err(error);
            }
        };
        Ok(WindowsPreferences {
            contrast,
            effects,
            contrast_token,
            effects_token,
        })
    };
    match setup() {
        Ok(observers) => Some(observers),
        Err(_) => {
            super::warning();
            None
        }
    }
}
