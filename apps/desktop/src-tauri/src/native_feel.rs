//! Native materials are opt-in after explicit accessibility and API success.
//! An opaque web surface remains the fallback, including on Linux.
//! Unavailable diagnostic output must not interrupt fallback or event delivery.

#![deny(clippy::print_stderr)]

mod platform;
mod policy;

use std::sync::Mutex;

use crate::diagnostics::application_message;
use policy::Appearance;
use tauri::{AppHandle, Emitter, Listener, Manager};

#[derive(Default)]
struct Cache {
    requested: Option<Appearance>,
    // Actual native layer survives failed cleanup; the web surface stays opaque.
    applied: Appearance,
    surface: Appearance,
    accent: Option<Option<[f64; 3]>>,
}

impl Cache {
    fn material(
        &mut self,
        requested: Appearance,
        apply: impl FnOnce() -> Result<Appearance, ()>,
        clear: impl FnOnce(Appearance) -> Result<(), ()>,
    ) -> Option<Result<Appearance, ()>> {
        if self.requested == Some(requested) {
            return None;
        }
        let result = if requested == Appearance::Opaque {
            if self.applied == Appearance::Opaque {
                Ok(())
            } else {
                clear(self.applied)
            }
            .map(|()| Appearance::Opaque)
        } else {
            // A failed clear may have left a layer behind. Never stack another.
            if self.applied == Appearance::Opaque {
                apply()
            } else {
                clear(self.applied).and_then(|()| {
                    self.applied = Appearance::Opaque;
                    apply()
                })
            }
        };
        self.requested = result.as_ref().ok().map(|_| requested);
        if let Ok(actual) = result {
            self.applied = actual;
        }
        self.surface = result.unwrap_or(Appearance::Opaque);
        Some(result)
    }

    fn accent_changed(&mut self, accent: Option<[f64; 3]>, replay: bool) -> bool {
        let changed = replay || self.accent != Some(accent);
        self.accent = Some(accent);
        changed
    }
}

fn warning() {
    // Native errors may contain handles or paths. Never print their payloads.
    application_message("native appearance is unavailable; using opaque surface");
}

/// Configure native and WebView transparency together before creating windows.
pub(crate) fn configure(context: &mut tauri::Context<tauri::Wry>) {
    if cfg!(any(target_os = "macos", target_os = "windows"))
        && let Some(window) = context
            .config_mut()
            .app
            .windows
            .iter_mut()
            .find(|window| window.label == "main")
    {
        window.transparent = true;
    }
}

/// Before the event loop, so an early `native_appearance` call or picker
/// presentation finds the cache and platform preferences.
pub(crate) fn manage(app: &AppHandle) {
    app.manage(Mutex::new(Cache::default()));
    platform::manage(app);
}

pub(crate) fn install(app: &AppHandle) {
    platform::watch(app);
    let handle = app.clone();
    app.listen("picker:shown", move |_| schedule(&handle));
}

fn schedule(app: &AppHandle) {
    let handle = app.clone();
    if app
        .run_on_main_thread(move || {
            refresh(&handle, false);
        })
        .is_err()
    {
        warning();
    }
}

/// Initial command runs asynchronously; native mutations stay on the main thread.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn native_appearance(app: AppHandle) -> String {
    let (send, mut receive) = tauri::async_runtime::channel(1);
    let handle = app.clone();
    if app
        .run_on_main_thread(move || {
            let _ = send.try_send(refresh(&handle, true));
        })
        .is_err()
    {
        warning();
        return Appearance::Opaque.name().to_owned();
    }
    receive.recv().await.unwrap_or(Appearance::Opaque).name().to_owned()
}

/// Refresh synchronously on the main thread before presentation, including
/// changes while hidden. All show entrypoints must dispatch before calling:
/// native material operations can synchronously query the main-thread HWND.
pub(crate) fn before_show(app: &AppHandle) {
    refresh(app, false);
}

fn refresh(app: &AppHandle, replay: bool) -> Appearance {
    let Some(window) = app.get_webview_window("main") else {
        return Appearance::Opaque;
    };
    // WKWebView transparency cannot be enabled by changing only its background
    // after creation. Refuse material mode when creation wasn't transparent.
    let transparent = app
        .config()
        .app
        .windows
        .iter()
        .any(|config| config.label == "main" && config.transparent);
    let material = platform::material();
    let preferences = if transparent { platform::preferences(app) } else { None };
    let requested = if transparent {
        policy::requested(material, preferences)
    } else {
        Appearance::Opaque
    };
    let cache = app.state::<Mutex<Cache>>();
    let Ok(mut cache) = cache.lock() else {
        warning();
        return Appearance::Opaque;
    };
    // Accent changes and query replay are independent of material success and
    // cache state, including opaque mode before JavaScript has subscribed.
    let accent = platform::accent();
    if cache.accent_changed(accent, replay) && window.emit("accent:changed", accent).is_err() {
        warning();
    }
    let previous = cache.surface;
    let Some(applied) = cache.material(
        requested,
        || platform::apply(&window),
        |mode| platform::clear(&window, mode),
    ) else {
        return cache.surface;
    };
    let next = applied.unwrap_or_else(|()| {
        warning();
        Appearance::Opaque
    });
    // Initial state and policy changes produce one fixed, allowlisted line in
    // the existing native startup artifact. Success is an API observation,
    // not proof of a visible material or accessibility-toggle acceptance.
    application_message(&format!(
        "native appearance mode={} reason={}",
        next.name(),
        policy::evidence_reason(material, preferences, transparent, next)
    ));
    if previous != next && window.emit("appearance:changed", next.name()).is_err() {
        warning();
    }
    next
}

#[cfg(test)]
mod tests {
    use super::{
        Appearance::{Acrylic, LiquidGlass, Opaque, Vibrancy},
        Cache,
    };

    #[test]
    fn failed_clear_preserves_actual_material_and_retries_the_same_request() {
        for actual in [LiquidGlass, Vibrancy, Acrylic] {
            let mut cache = Cache {
                applied: actual,
                ..Default::default()
            };
            assert_eq!(
                cache.material(Opaque, || panic!("must clear"), |_| Err(())),
                Some(Err(()))
            );
            assert_eq!(cache.applied, actual);
            assert_eq!(cache.requested, None);
            assert_eq!(
                cache.material(
                    Opaque,
                    || panic!("must clear"),
                    |mode| {
                        assert_eq!(mode, actual);
                        Ok(())
                    }
                ),
                Some(Ok(Opaque))
            );
            assert_eq!(cache.applied, Opaque);
        }
    }
}
