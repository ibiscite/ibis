//! Debug evidence entry point. The release application does not compile this module.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::ipc::CapabilityBuilder;
use tauri::{AppHandle, Listener, Manager, Url, WebviewWindow};

const STATES: [&str; 15] = [
    "first-run",
    "loading",
    "empty",
    "no-results",
    "many-results",
    "long-titles",
    "rtl",
    "missing-file",
    "invalid-settings",
    "damaged-library",
    "copy-confirmation",
    "large-text",
    "dark",
    "high-contrast",
    "copied-notice",
];

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiagnosticPayload {
    state: String,
    stage: String,
}

fn diagnostic_stage(payload: &str, expected: &str) -> Option<&'static str> {
    if payload.len() > 128 {
        return None;
    }
    let payload: DiagnosticPayload = serde_json::from_str(payload).ok()?;
    if payload.state != expected {
        return None;
    }
    match payload.stage.as_str() {
        "frontend-mounted" => Some("frontend-mounted"),
        "effects-settled" => Some("effects-settled"),
        "rendered" => Some("rendered"),
        _ => None,
    }
}

fn selected_document(url: &Url, expected: &str) -> bool {
    url.path() == "/gallery.html"
        && url.query_pairs().count() == 1
        && url
            .query_pairs()
            .any(|(key, value)| key == "state" && value == expected)
}

struct ReadyAck {
    state: String,
    path: PathBuf,
    acknowledged: AtomicBool,
}

impl ReadyAck {
    fn new(window: &str, state: Option<&str>, path: Option<PathBuf>) -> Option<Self> {
        let state = state.filter(|state| STATES.contains(state))?;
        let path = path?;
        if window != "main" || !path.is_absolute() || path.file_name()?.to_str()? != format!("{state}.ready") {
            return None;
        }
        Some(Self {
            state: state.to_string(),
            path,
            acknowledged: AtomicBool::new(false),
        })
    }

    fn accept(&self, payload: &str) -> std::io::Result<bool> {
        if payload.len() > 128
            || serde_json::from_str::<String>(payload).ok().as_deref() != Some(self.state.as_str())
            || self.acknowledged.swap(true, Ordering::SeqCst)
        {
            return Ok(false);
        }
        eprintln!("ibis-gallery: event-accepted");
        // The renderer supplies only a fixed state. Its destination came from
        // the capture process environment; no web command accepts a file path.
        let temporary = self.path.with_extension("ready.tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let written = (|| {
            writeln!(file, "{{\"state\":\"{}\"}}", self.state)?;
            drop(file);
            std::fs::rename(&temporary, &self.path)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        written.map(|()| true)
    }
}

/// Select the first document before Tauri creates its webviews. Navigating in
/// setup can race the initial WebView2 load and leave a blank Windows picker.
pub(crate) fn configure<R: tauri::Runtime>(context: &mut tauri::Context<R>) -> tauri::Result<()> {
    let enabled = std::env::var("IBIS_GALLERY").ok();
    let state = std::env::var("IBIS_GALLERY_STATE").ok();
    configure_windows(
        &mut context.config_mut().app.windows,
        enabled.as_deref(),
        state.as_deref(),
    )
}

fn configure_windows(
    windows: &mut [tauri::utils::config::WindowConfig],
    enabled: Option<&str>,
    state: Option<&str>,
) -> tauri::Result<()> {
    if enabled != Some("1") {
        return Ok(());
    }
    // Use the shared encoder, then give Tauri a relative app URL. Tauri joins
    // this with its own platform/dev origin when constructing each webview.
    let origin = Url::parse("tauri://localhost/").map_err(tauri::Error::InvalidUrl)?;
    if let Some(destination) = gallery_url(origin, enabled, state) {
        let mut relative = destination.path().trim_start_matches('/').to_string();
        if let Some(query) = destination.query() {
            relative.push('?');
            relative.push_str(query);
        }
        for window in windows.iter_mut().filter(|window| window.label == "main") {
            window.url = tauri::WebviewUrl::App(PathBuf::from(&relative));
        }
    }
    Ok(())
}

/// Called during setup, and only inside the debug gallery opt-in block.
/// Normal and release windows retain their default event permissions.
pub(crate) fn install_readiness(
    app: &AppHandle,
    window: &WebviewWindow,
    selected: Option<&str>,
    ready_file: Option<PathBuf>,
) -> tauri::Result<()> {
    let Some(ready) = ReadyAck::new(window.label(), selected, ready_file) else {
        return Ok(());
    };
    let ready = Arc::new(ready);
    app.add_capability(
        CapabilityBuilder::new("debug-gallery-rendered")
            .window("main")
            .permission("core:event:allow-emit-to"),
    )
    .inspect_err(|_| eprintln!("ibis-gallery: install-failed"))?;
    let expected = ready.state.clone();
    window.listen("gallery:stage", move |event| {
        if let Some(stage) = diagnostic_stage(event.payload(), &expected) {
            eprintln!("ibis-gallery: {stage}");
        }
    });
    let accepted = Arc::clone(&ready);
    window.listen("gallery:rendered", move |event| {
        if accepted.accept(event.payload()).is_err() {
            eprintln!("ibis-gallery: write-failed");
        }
    });
    eprintln!("ibis-gallery: installed");
    // Inspect only the expected path/query, never print a URL. The bounded
    // observations distinguish failed navigation from a renderer/IPC failure.
    let observed = window.clone();
    std::thread::spawn(move || {
        for seconds in [1, 3, 6] {
            std::thread::sleep(Duration::from_secs(seconds));
            if ready.acknowledged.load(Ordering::SeqCst) {
                return;
            }
            match observed.url() {
                Ok(url) if selected_document(&url, &ready.state) => {
                    eprintln!("ibis-gallery: navigated");
                    return;
                }
                Ok(_) => eprintln!("ibis-gallery: navigation-pending"),
                Err(_) => eprintln!("ibis-gallery: navigation-unavailable"),
            }
        }
    });
    Ok(())
}

/// Change only the local document and its query, retaining the webview's origin
/// for both Tauri's custom protocol and its development HTTP server.
pub(crate) fn gallery_url(mut current: Url, enabled: Option<&str>, state: Option<&str>) -> Option<Url> {
    if enabled != Some("1") {
        return None;
    }
    current.set_path("/gallery.html");
    current.set_query(None);
    current.set_fragment(None);
    if let Some(state) = state {
        current.query_pairs_mut().append_pair("state", state);
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: &str) -> Url {
        Url::parse(value).unwrap_or_else(|error| panic!("synthetic URL was invalid: {error}"))
    }

    fn synthetic_config() -> tauri::Config {
        serde_json::from_str(include_str!("../tauri.conf.json"))
            .unwrap_or_else(|error| panic!("invalid synthetic app config: {error}"))
    }

    #[test]
    fn initial_gallery_configuration_changes_only_the_main_url() {
        let mut config = synthetic_config();
        let mut other = config.app.windows[0].clone();
        other.label = "other".into();
        other.url = tauri::WebviewUrl::App("other.html?keep=1".into());
        config.app.windows.push(other);
        let original_url = config.app.windows[0].url.clone();
        let original = serde_json::to_value(&config).unwrap_or_else(|e| panic!("{e}"));
        configure_windows(&mut config.app.windows, Some("1"), Some("first-run")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            config.app.windows[0].url,
            tauri::WebviewUrl::App("gallery.html?state=first-run".into())
        );
        // Includes WebView2 network arguments, placement and every other option.
        config.app.windows[0].url = original_url;
        assert_eq!(
            serde_json::to_value(&config).unwrap_or_else(|e| panic!("{e}")),
            original
        );
    }

    #[test]
    fn disabled_initial_gallery_configuration_preserves_all_windows() {
        for enabled in [None, Some(""), Some("0"), Some("true"), Some(" 1"), Some("1 ")] {
            let mut config = synthetic_config();
            config.app.windows[0].url = tauri::WebviewUrl::App("original.html?keep=1#fragment".into());
            let original = serde_json::to_value(&config).unwrap_or_else(|e| panic!("{e}"));
            configure_windows(&mut config.app.windows, enabled, Some("loading")).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(
                serde_json::to_value(&config).unwrap_or_else(|e| panic!("{e}")),
                original
            );
        }
    }

    #[test]
    fn gallery_configuration_without_a_main_window_changes_nothing() {
        let mut config = synthetic_config();
        config.app.windows[0].label = "other".into();
        let original = serde_json::to_value(&config).unwrap_or_else(|e| panic!("{e}"));
        configure_windows(&mut config.app.windows, Some("1"), Some("loading")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            serde_json::to_value(&config).unwrap_or_else(|e| panic!("{e}")),
            original
        );
    }

    #[test]
    fn initial_gallery_relative_url_joins_all_tauri_origins_with_one_encoded_state() {
        for state in [
            "loading",
            "//other.invalid/?state=empty#x",
            "loading&state=empty",
            "日本語 + % ? & #",
            "",
        ] {
            let mut config = synthetic_config();
            configure_windows(&mut config.app.windows, Some("1"), Some(state)).unwrap_or_else(|e| panic!("{e}"));
            let tauri::WebviewUrl::App(relative) = &config.app.windows[0].url else {
                panic!("gallery must remain a Tauri app document");
            };
            for origin in [
                "tauri://localhost/",
                "http://tauri.localhost/",
                "https://tauri.localhost/",
                "http://localhost:5173/",
            ] {
                let base = parse(origin);
                let joined = base.join(&relative.to_string_lossy()).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(joined.scheme(), base.scheme());
                assert_eq!(joined.host_str(), base.host_str());
                assert_eq!(joined.port(), base.port());
                assert_eq!(joined.path(), "/gallery.html");
                assert_eq!(joined.fragment(), None);
                assert_eq!(
                    joined.query_pairs().collect::<Vec<_>>(),
                    [("state".into(), state.into())]
                );
            }
        }
        let mut config = synthetic_config();
        configure_windows(&mut config.app.windows, Some("1"), None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(config.app.windows[0].url, tauri::WebviewUrl::App("gallery.html".into()));
    }

    #[test]
    fn diagnostic_stages_require_the_selected_state_and_fixed_payloads() {
        assert_eq!(
            diagnostic_stage(r#"{"state":"loading","stage":"rendered"}"#, "loading"),
            Some("rendered")
        );
        for payload in [
            r#"{"state":"empty","stage":"rendered"}"#,
            r#"{"state":"loading","stage":"private payload"}"#,
            r#"{"state":"loading","stage":"rendered","path":"private"}"#,
            "not JSON",
        ] {
            assert_eq!(diagnostic_stage(payload, "loading"), None);
        }
    }

    #[test]
    fn native_navigation_diagnostics_require_the_exact_selected_gallery_document() {
        assert!(selected_document(
            &parse("https://tauri.localhost/gallery.html?state=loading"),
            "loading"
        ));
        for other in [
            "https://tauri.localhost/index.html?state=loading",
            "https://tauri.localhost/gallery.html?state=empty",
            "https://tauri.localhost/gallery.html?state=loading&state=empty",
            "https://tauri.localhost/gallery.html",
        ] {
            assert!(!selected_document(&parse(other), "loading"));
        }
    }

    #[test]
    fn readiness_requires_the_main_window_known_state_and_a_configured_destination() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("loading.ready");
        assert!(ReadyAck::new("other", Some("loading"), Some(path.clone())).is_none());
        assert!(ReadyAck::new("main", Some("../loading"), Some(path.clone())).is_none());
        assert!(ReadyAck::new("main", Some("loading"), None).is_none());
        assert!(ReadyAck::new("main", None, Some(path.clone())).is_none());
        assert!(ReadyAck::new("main", Some("empty"), Some(path.clone())).is_none());
        assert!(ReadyAck::new("main", Some("loading"), Some(PathBuf::from("loading.ready"))).is_none());
        assert!(!path.exists());
    }

    #[test]
    fn only_the_selected_render_writes_one_atomic_acknowledgment() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("loading.ready");
        let ready =
            ReadyAck::new("main", Some("loading"), Some(path.clone())).unwrap_or_else(|| panic!("no ready target"));
        for invalid in ["not JSON", "\"empty\"", "{\"state\":\"loading\",\"path\":\"other\"}"] {
            assert!(!ready.accept(invalid).unwrap_or_else(|e| panic!("{e}")));
            assert!(!path.exists());
        }
        assert!(ready.accept("\"loading\"").unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}")),
            "{\"state\":\"loading\"}\n"
        );
        assert!(!path.with_extension("ready.tmp").exists());
        std::fs::write(&path, "duplicate sentinel").unwrap_or_else(|e| panic!("{e}"));
        assert!(!ready.accept("\"loading\"").unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}")),
            "duplicate sentinel"
        );
    }

    #[test]
    fn gallery_navigation_preserves_custom_protocol_and_development_origins() {
        for original in [
            "tauri://localhost/index.html?stale=1#old",
            "http://tauri.localhost/index.html?stale=1#old",
            "https://tauri.localhost/index.html?stale=1#old",
            "http://localhost:5173/index.html?stale=1#old",
        ] {
            let current = parse(original);
            let destination =
                gallery_url(current.clone(), Some("1"), None).unwrap_or_else(|| panic!("gallery was not enabled"));
            assert_eq!(destination.scheme(), current.scheme());
            assert_eq!(destination.host_str(), current.host_str());
            assert_eq!(destination.port(), current.port());
            assert_eq!(destination.path(), "/gallery.html");
            assert_eq!(destination.query(), None);
            assert_eq!(destination.fragment(), None);
        }
    }

    #[test]
    fn state_is_one_encoded_query_value_and_cannot_change_the_destination() {
        for state in [
            "loaded",
            "//attacker.invalid/other?state=empty#fragment",
            "loaded&state=error&extra=1",
            "../index.html?\n#<script>alert(1)</script>",
            "日本語 + % ? & #",
            "",
        ] {
            let destination = gallery_url(parse("tauri://localhost/"), Some("1"), Some(state))
                .unwrap_or_else(|| panic!("gallery was not enabled"));
            assert_eq!(destination.scheme(), "tauri");
            assert_eq!(destination.host_str(), Some("localhost"));
            assert_eq!(destination.path(), "/gallery.html");
            assert_eq!(destination.fragment(), None);
            let pairs: Vec<_> = destination.query_pairs().collect();
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs[0].0, "state");
            assert_eq!(pairs[0].1, state);
        }
    }

    #[test]
    fn only_the_exact_gallery_opt_in_changes_navigation() {
        for enabled in [None, Some(""), Some("0"), Some("true"), Some(" 1"), Some("1 ")] {
            assert!(gallery_url(parse("http://localhost:5173/"), enabled, Some("loaded")).is_none());
        }
    }
}
