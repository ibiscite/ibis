//! Native materials are opt-in after explicit accessibility and API success.
//! An opaque web surface remains the fallback, including on Linux.

mod platform;
mod policy;

use std::sync::Mutex;

use policy::Appearance;
use tauri::{AppHandle, Emitter, Listener, Manager};

#[derive(Default)]
struct Cache {
    requested: Option<Appearance>,
    applied: Appearance,
}

fn warning() {
    // Native errors may contain handles or paths. Never print their payloads.
    eprintln!("ibis: native appearance is unavailable; using opaque surface");
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

pub(crate) fn install(app: &AppHandle) {
    app.manage(Mutex::new(Cache::default()));
    platform::watch(app);
    let handle = app.clone();
    app.listen("picker:shown", move |_| schedule(&handle));
}

fn schedule(app: &AppHandle) {
    let handle = app.clone();
    if app
        .run_on_main_thread(move || {
            refresh(&handle);
        })
        .is_err()
    {
        warning();
    }
}

/// Initial command runs asynchronously; native mutations stay on the main thread.
#[tauri::command]
#[specta::specta]
pub(crate) async fn native_appearance(app: AppHandle) -> String {
    let (send, mut receive) = tauri::async_runtime::channel(1);
    let handle = app.clone();
    if app
        .run_on_main_thread(move || {
            let _ = send.try_send(refresh(&handle));
        })
        .is_err()
    {
        warning();
        return Appearance::Opaque.name().to_owned();
    }
    receive.recv().await.unwrap_or(Appearance::Opaque).name().to_owned()
}

fn refresh(app: &AppHandle) -> Appearance {
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
    if cache.requested == Some(requested) {
        return cache.applied;
    }
    cache.requested = Some(requested);
    let next = if requested == Appearance::Opaque {
        if cache.applied != Appearance::Opaque && platform::clear(&window).is_err() {
            warning();
        }
        Appearance::Opaque
    } else {
        let success = platform::apply(&window).is_ok();
        if !success {
            warning();
        }
        policy::applied(requested, success)
    };
    // Initial state and policy changes produce one fixed, allowlisted line in
    // the existing native startup artifact. Success is an API observation,
    // not proof of a visible material or accessibility-toggle acceptance.
    eprintln!(
        "ibis: native appearance mode={} reason={}",
        next.name(),
        policy::evidence_reason(material, preferences, transparent, next)
    );
    if cache.applied != next {
        cache.applied = next;
        if window.emit("appearance:changed", next.name()).is_err() {
            warning();
        }
    }
    next
}
