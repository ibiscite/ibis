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
            refresh(&handle, false);
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
            let _ = send.try_send(refresh(&handle, true));
        })
        .is_err()
    {
        warning();
        return Appearance::Opaque.name().to_owned();
    }
    receive.recv().await.unwrap_or(Appearance::Opaque).name().to_owned()
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
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

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

    #[test]
    fn failed_apply_retries_on_the_next_refresh() {
        let mut cache = Cache::default();
        assert_eq!(
            cache.material(LiquidGlass, || Err(()), |_| panic!("no layer")),
            Some(Err(()))
        );
        assert_eq!(cache.requested, None);
        assert_eq!(
            cache.material(LiquidGlass, || Ok(Vibrancy), |_| panic!("no layer")),
            Some(Ok(Vibrancy))
        );
        assert_eq!(cache.applied, Vibrancy);
    }

    #[test]
    fn reenabled_material_cannot_stack_over_a_failed_clear() {
        for actual in [LiquidGlass, Vibrancy] {
            let mut cache = Cache {
                applied: actual,
                surface: actual,
                ..Default::default()
            };
            for requested in [Opaque, Opaque, LiquidGlass] {
                assert_eq!(
                    cache.material(
                        requested,
                        || panic!("retained layer"),
                        |mode| {
                            assert_eq!(mode, actual);
                            Err(())
                        }
                    ),
                    Some(Err(()))
                );
                assert_eq!(cache.applied, actual);
                assert_eq!(cache.surface, Opaque);
                assert_eq!(cache.requested, None);
            }
            assert_eq!(
                cache.material(
                    LiquidGlass,
                    || Ok(actual),
                    |mode| {
                        assert_eq!(mode, actual);
                        Ok(())
                    }
                ),
                Some(Ok(actual))
            );
            assert_eq!(cache.applied, actual);
            assert_eq!(cache.surface, actual);
            assert_eq!(
                cache.material(LiquidGlass, || panic!("already applied"), |_| panic!("already applied")),
                None
            );
        }
    }

    #[test]
    fn material_cache_clears_actual_layer_and_skips_unchanged_requests() {
        for actual in [super::Appearance::LiquidGlass, super::Appearance::Vibrancy] {
            let mut cache = super::Cache {
                requested: Some(super::Appearance::LiquidGlass),
                applied: actual,
                ..Default::default()
            };
            assert_eq!(
                cache.material(
                    super::Appearance::LiquidGlass,
                    || panic!("unchanged request"),
                    |_| panic!("unchanged request")
                ),
                None
            );
            assert_eq!(
                cache.material(
                    super::Appearance::Opaque,
                    || panic!("opacity must not apply"),
                    |mode| {
                        assert_eq!(mode, actual);
                        Ok(())
                    }
                ),
                Some(Ok(super::Appearance::Opaque))
            );
            assert_eq!(
                cache.material(
                    super::Appearance::Opaque,
                    || panic!("duplicate request"),
                    |_| panic!("duplicate clear")
                ),
                None
            );
        }
    }

    #[test]
    fn accent_changes_and_query_replays_ignore_unchanged_opaque_material() {
        let mut cache = super::Cache {
            requested: Some(super::Appearance::Opaque),
            ..Default::default()
        };
        let yellow = Some([1.0, 0.72, 0.0]);
        assert!(cache.accent_changed(yellow, false));
        assert!(!cache.accent_changed(yellow, false));
        assert!(cache.accent_changed(yellow, true));
        assert!(cache.accent_changed(None, false));
        assert!(cache.accent_changed(None, true));
        assert_eq!(cache.applied, super::Appearance::Opaque);
    }

    #[test]
    fn appearance_warning_survives_closed_stderr() {
        const CHILD: &str = "IBIS_TEST_APPEARANCE_STDERR";
        if std::env::var_os(CHILD).as_deref() == Some(std::ffi::OsStr::new("1")) {
            let mut gate = [0];
            std::io::stdin()
                .read_exact(&mut gate)
                .unwrap_or_else(|_| panic!("missing diagnostic test gate"));
            assert_eq!(gate, [b'g']);
            super::warning();
            #[cfg(target_os = "macos")]
            crate::native_corners::test_warning();
            return;
        }

        for closed in [false, true] {
            let mut child =
                Command::new(std::env::current_exe().unwrap_or_else(|_| panic!("test executable unavailable")))
                    .args([
                        "--exact",
                        "native_feel::tests::appearance_warning_survives_closed_stderr",
                        "--nocapture",
                    ])
                    .env(CHILD, "1")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap_or_else(|_| panic!("cannot start diagnostic test child"));
            let mut stderr = child.stderr.take();
            if closed {
                drop(stderr.take());
            }
            // For the failure case, close the reader before releasing the actual warning.
            let released = child
                .stdin
                .take()
                .ok_or_else(|| std::io::Error::other("diagnostic test input unavailable"))
                .and_then(|mut input| input.write_all(b"g"));
            if released.is_err() {
                let _ = child.kill();
                let _ = child.wait();
                panic!("cannot release diagnostic test child");
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        child
                            .wait()
                            .unwrap_or_else(|_| panic!("cannot reap diagnostic test child"));
                        assert!(
                            status.success(),
                            "appearance warning interrupted recovery (closed stderr: {closed}, status: {status})"
                        );
                        break;
                    }
                    Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("diagnostic test child did not finish within five seconds");
                    }
                }
            }
            if let Some(mut stderr) = stderr {
                let mut output = Vec::new();
                stderr
                    .read_to_end(&mut output)
                    .unwrap_or_else(|_| panic!("cannot read diagnostic test output"));
                let expected = b"ibis: native appearance is unavailable; using opaque surface\n".as_slice();
                #[cfg(target_os = "macos")]
                let expected = [
                    expected,
                    b"ibis: native picker corners unavailable: the picker content layer is unavailable; using rectangular window\n",
                ]
                .concat();
                assert_eq!(output, expected);
            }
        }
    }
}
