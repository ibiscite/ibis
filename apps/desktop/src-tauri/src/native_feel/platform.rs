use super::policy::{Appearance, Preferences};
use tauri::{AppHandle, WebviewWindow};

pub(super) fn accent() -> Option<[f64; 3]> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSColor, NSColorSpace};
        // Component getters raise for non-RGB colours. Convert first using the
        // pinned safe API; never assume the dynamic accent's native space.
        let color = NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
        let rgb = [color.redComponent(), color.greenComponent(), color.blueComponent()];
        rgb.iter()
            .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
            .then_some(rgb)
    }
    #[cfg(not(target_os = "macos"))]
    None
}

pub(super) fn material() -> Appearance {
    #[cfg(target_os = "macos")]
    return Appearance::LiquidGlass;
    #[cfg(target_os = "windows")]
    {
        let version = windows::System::Profile::AnalyticsInfo::VersionInfo()
            .and_then(|info| info.DeviceFamilyVersion())
            .ok();
        super::policy::windows_material(version.as_ref().map(|v| v.to_string()).as_deref())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Appearance::Opaque
}

pub(super) fn apply(window: &WebviewWindow) -> Result<Appearance, ()> {
    #[cfg(target_os = "macos")]
    return super::policy::macos_material(
        window_vibrancy::apply_liquid_glass(
            window,
            window_vibrancy::LiquidGlassOptions::default().radius(crate::native_corners::CORNER_RADIUS_POINTS),
        )
        .map_err(|error| match error {
            window_vibrancy::Error::UnsupportedPlatformVersion(_) => super::policy::GlassFailure::Unsupported,
            _ => super::policy::GlassFailure::Failed,
        }),
        || {
            window_vibrancy::apply_vibrancy(
                window,
                window_vibrancy::NSVisualEffectMaterial::Popover,
                Some(window_vibrancy::NSVisualEffectState::Active),
                None,
            )
            .map_err(|_| ())
        },
    );
    #[cfg(target_os = "windows")]
    return window_vibrancy::apply_acrylic(window, None)
        .map(|()| Appearance::Acrylic)
        .map_err(|_| ());
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = window;
        Err(())
    }
}

pub(super) fn clear(window: &WebviewWindow, applied: Appearance) -> Result<(), ()> {
    #[cfg(target_os = "macos")]
    return match applied {
        Appearance::LiquidGlass => window_vibrancy::clear_liquid_glass(window).map(|_| ()).map_err(|_| ()),
        _ => window_vibrancy::clear_vibrancy(window).map(|_| ()).map_err(|_| ()),
    };
    #[cfg(target_os = "windows")]
    {
        let _ = applied;
        window_vibrancy::clear_acrylic(window).map_err(|_| ())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (window, applied);
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
            reduce_transparency: !effects.AdvancedEffectsEnabled().ok()?
                || !super::policy::energy_saver_allows_effects(
                    windows::System::Power::PowerManager::EnergySaverStatus().ok()?.0,
                )?,
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
                            super::refresh(&task, false);
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
    energy_token: Option<i64>,
}

#[cfg(target_os = "windows")]
impl Drop for WindowsPreferences {
    fn drop(&mut self) {
        let contrast = self.contrast.RemoveHighContrastChanged(self.contrast_token);
        let effects = self.effects.RemoveAdvancedEffectsEnabledChanged(self.effects_token);
        let energy = self
            .energy_token
            .map(windows::System::Power::PowerManager::RemoveEnergySaverStatusChanged)
            .transpose();
        if contrast.is_err() || effects.is_err() || energy.is_err() {
            super::warning();
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_watch(app: &AppHandle) -> Option<WindowsPreferences> {
    use windows::{
        Foundation::{EventHandler, TypedEventHandler},
        System::Power::PowerManager,
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
        // Own both existing registrations before adding the static observer:
        // any later setup error drops them, and no theme refresh installs more.
        let mut observers = WindowsPreferences {
            contrast,
            effects,
            contrast_token,
            effects_token,
            energy_token: None,
        };
        let handle = app.clone();
        observers.energy_token = Some(PowerManager::EnergySaverStatusChanged(&EventHandler::<
            windows::core::IInspectable,
        >::new(move |_, _| {
            super::schedule(&handle);
            Ok(())
        }))?);
        Ok(observers)
    };
    match setup() {
        Ok(observers) => Some(observers),
        Err(_) => {
            super::warning();
            None
        }
    }
}
