//! Saving choices made in the picker to the settings file.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ibis_core::cite::KeyFormat;
use ibis_core::settings::{self, CitationStyle, Density};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::library::Library;

/// Settings errors can contain user-entered values or OS details. Only these
/// fixed descriptions reach diagnostics or messages about failed saves.
pub(crate) fn settings_problem(problem: &settings::SettingsError) -> &'static str {
    match problem {
        settings::SettingsError::Unreadable(_) => "settings.json could not be read",
        settings::SettingsError::Invalid(_) => "settings.json is not valid",
        settings::SettingsError::InvalidZoteroLibraryEntry { .. } => {
            "settings.json has an invalid Zotero library entry"
        }
        settings::SettingsError::NewerVersion(_) => "settings.json was written by a newer version of Ibis",
        settings::SettingsError::Backup(error) => match error.as_ref() {
            settings::SettingsError::NewerVersion(_) => "settings.json.bak was written by a newer version of Ibis",
            settings::SettingsError::Invalid(_) | settings::SettingsError::InvalidZoteroLibraryEntry { .. } => {
                "settings.json.bak is not valid"
            }
            _ => "settings.json.bak could not be read",
        },
    }
}

/// Interface and failed-save text may include a typed position, never raw
/// serde or operating-system details. Diagnostics keep the fixed description.
fn settings_message(problem: &settings::SettingsError) -> String {
    match problem {
        settings::SettingsError::InvalidZoteroLibraryEntry { .. } => {
            problem.to_string().replacen("the settings file", "settings.json", 1)
        }
        settings::SettingsError::Backup(error) => {
            settings_message(error).replacen("settings.json", "settings.json.bak", 1)
        }
        _ => settings_problem(problem).to_string(),
    }
}

/// This notice is for the interface only: its settings path must not be logged.
pub(crate) fn settings_notice(path: &Path, loaded: &settings::Loaded) -> Option<String> {
    loaded.problem.as_ref().map(|problem| {
        if loaded.primary_read == settings::PrimaryRead::Missing && loaded.from_backup {
            return format!(
                "settings.json is missing; using the backup. The next save will restore settings.json and keep the backup unchanged. Settings file: {}",
                path.display()
            );
        }
        let recovery = if loaded.from_backup { "the backup" } else { "defaults" };
        let recoverable = match problem {
            settings::SettingsError::Invalid(_) | settings::SettingsError::InvalidZoteroLibraryEntry { .. } => true,
            settings::SettingsError::Unreadable(error) => error.kind() == std::io::ErrorKind::InvalidData,
            settings::SettingsError::NewerVersion(_) | settings::SettingsError::Backup(_) => false,
        };
        let saving = if loaded.from_backup && recoverable {
            "The next save will keep the damaged file as settings.json.invalid-N and restore settings from the backup."
        } else {
            "Changes are not being saved while this problem remains."
        };
        let problem_path = match problem {
            settings::SettingsError::Backup(_) => path.with_extension("json.bak"),
            _ => path.to_path_buf(),
        };
        format!(
            "{}; using {recovery}. {saving} Settings file: {}",
            settings_message(problem),
            problem_path.display()
        )
    })
}

/// Where the settings file is, for saving choices made in the picker.
#[derive(Clone)]
pub(crate) struct SettingsFile {
    pub(crate) path: PathBuf,
    /// Held for each whole read, change and save, so two saves never interleave.
    writing: Arc<Mutex<()>>,
}

impl SettingsFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        SettingsFile {
            path,
            writing: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn update(&self, change: impl FnOnce(&mut settings::Settings)) -> Result<settings::Settings, String> {
        let _writing = self
            .writing
            .lock()
            .map_err(|_| "settings are unavailable".to_string())?;
        settings::update(&self.path, change).map_err(|error| settings_message(&error))
    }

    /// Refresh under the library lock so a reload cannot carry forward a stale
    /// recovery warning.
    pub(crate) fn refresh_notice(&self, library: &mut Library) {
        library.settings_notice = settings_notice(&self.path, &settings::load(&self.path));
    }
}

/// How the picker looks and what it formats with, remembered across restarts
/// (ADR 0020). A new key format is saved with the same preference change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub(crate) struct PickerPreferences {
    /// A style chosen in the format menu, or `null` to copy citation keys.
    pub(crate) citation_style: Option<CitationStyle>,
    pub(crate) density: Density,
    pub(crate) larger_text: bool,
    pub(crate) show_paste_preview: bool,
    /// The Ibis mark in place of the search glyph; off by default.
    pub(crate) search_mark: bool,
    /// Enter pastes into the app the picker was opened from; on by default.
    pub(crate) paste: bool,
}

impl PickerPreferences {
    fn from_settings(settings: &settings::Settings) -> Self {
        PickerPreferences {
            citation_style: settings.citation_style,
            density: settings.density.unwrap_or(Density::Comfortable),
            larger_text: settings.larger_text.unwrap_or(false),
            show_paste_preview: settings.show_paste_preview.unwrap_or(true),
            search_mark: settings.search_mark.unwrap_or(false),
            paste: settings.paste.unwrap_or(true),
        }
    }
}

/// The preferences the picker starts with.
#[tauri::command]
#[specta::specta]
pub(crate) fn picker_preferences(settings_file: State<'_, SettingsFile>) -> PickerPreferences {
    PickerPreferences::from_settings(&settings::load(&settings_file.path).settings)
}

/// Remember the picker preferences; other settings in the file are kept.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_picker_preferences<R: tauri::Runtime>(
    preferences: PickerPreferences,
    copy_format: Option<KeyFormat>,
    settings_file: State<'_, SettingsFile>,
    app: AppHandle<R>,
    window: tauri::Window<R>,
) -> Result<(), String> {
    let settings_file = settings_file.inner().clone();
    // A change made in Settings reaches the picker; the picker's own are already shown.
    let from_elsewhere = window.label() != "main";
    // Keep the whole locked read/change/flush off the command and async workers.
    tauri::async_runtime::spawn_blocking(move || {
        settings_file.save_preferences(preferences, copy_format)?;
        crate::library::refresh_settings_notice(&app, &settings_file);
        if from_elsewhere {
            crate::diagnostics::report(
                app.emit_to("main", "preferences:changed", preferences),
                crate::diagnostics::NativeIssue::PickerEvent,
            );
        }
        Ok(())
    })
    .await
    .map_err(|_| "picker preferences could not be saved".to_string())?
}

impl SettingsFile {
    fn save_preferences(
        &self,
        preferences: PickerPreferences,
        copy_format: Option<KeyFormat>,
    ) -> Result<settings::Settings, String> {
        self.update(|s| {
            if let Some(format) = copy_format {
                s.copy_format = Some(format);
            }
            s.citation_style = preferences.citation_style;
            // Defaults stay unwritten, so the file only names real choices.
            s.density = Some(preferences.density).filter(|d| *d != Density::Comfortable);
            s.larger_text = Some(preferences.larger_text).filter(|larger| *larger);
            s.show_paste_preview = Some(preferences.show_paste_preview).filter(|shown| !shown);
            s.search_mark = Some(preferences.search_mark).filter(|mark| *mark);
            s.paste = Some(preferences.paste).filter(|paste| !paste);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::settings::LibrarySetting;
    use tauri::Manager;

    fn preference_request(
        preferences: PickerPreferences,
        copy_format: Option<KeyFormat>,
    ) -> tauri::webview::InvokeRequest {
        tauri::webview::InvokeRequest {
            cmd: "set_picker_preferences".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(windows) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap_or_else(|e| panic!("invalid fixture URL: {e}")),
            body: tauri::ipc::InvokeBody::Json(serde_json::json!({
                "preferences": preferences, "copyFormat": copy_format,
            })),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        }
    }

    #[test]
    fn preference_dispatch_returns_while_writer_is_busy_and_save_keeps_current_settings() {
        use std::sync::mpsc;
        use std::time::Duration;

        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        std::fs::write(&path, r#"{"copy_format":"keys","future_choice":"before dispatch"}"#)
            .unwrap_or_else(|e| panic!("{e}"));
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
            .invoke_handler(tauri::generate_handler![set_picker_preferences])
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|e| panic!("cannot build preference fixture: {e}"));
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("cannot build window fixture: {e}"));
        let file = app.state::<SettingsFile>();
        let writing = file.writing.lock().unwrap_or_else(|_| panic!("writer lock poisoned"));
        let chosen = PickerPreferences {
            citation_style: None,
            density: Density::Spacious,
            larger_text: true,
            show_paste_preview: false,
            search_mark: false,
            paste: true,
        };
        let (dispatched, dispatch) = mpsc::channel();
        let (responded, response) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            window.as_ref().clone().on_message(
                preference_request(chosen, Some(KeyFormat::Typst)),
                Box::new(move |_, _, reply, _, _| {
                    let _ = responded.send(reply);
                }),
            );
            let _ = dispatched.send(());
        });
        let returned = dispatch.recv_timeout(Duration::from_secs(3));
        let pending = matches!(response.try_recv(), Err(mpsc::TryRecvError::Empty));
        // Simulate an external edit after dispatch, before the writer can read.
        let current = serde_json::json!({
            "schema_version": 2, "copy_format": "cite", "citation_style": "apa",
            "library": { "bibtex": "refs.bib" }, "shortcut": "Alt+Space", "update_checks": false,
            "future_choice": { "kept": [true, null, "参考"] },
        });
        let edited = std::fs::write(&path, current.to_string());
        drop(writing);
        let reply = response.recv_timeout(Duration::from_secs(3));
        // Release the lock and collect the reply even on a regression.
        assert!(returned.is_ok(), "IPC dispatch blocked behind a settings writer");
        assert!(pending, "save must wait for the existing writer");
        edited.unwrap_or_else(|e| panic!("{e}"));
        let tauri::ipc::InvokeResponse::Ok(body) = reply.unwrap_or_else(|e| panic!("save did not finish: {e}")) else {
            panic!("save unexpectedly failed");
        };
        assert_eq!(
            body.deserialize::<serde_json::Value>()
                .unwrap_or_else(|e| panic!("invalid reply: {e}")),
            serde_json::Value::Null
        );
        thread.join().unwrap_or_else(|_| panic!("dispatch thread panicked"));
        let loaded = settings::load(&path);
        assert!(loaded.problem.is_none());
        assert_eq!(PickerPreferences::from_settings(&loaded.settings), chosen);
        assert_eq!(loaded.settings.copy_format, Some(KeyFormat::Typst));
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default()).unwrap_or_else(|e| panic!("{e}"));
        for field in ["library", "shortcut", "update_checks", "future_choice"] {
            assert_eq!(saved[field], current[field], "queued save keeps the current {field}");
        }
    }

    #[test]
    fn preference_recovery_save_refreshes_and_publishes_the_settings_notice() {
        use std::sync::mpsc;
        use std::time::Duration;
        use tauri::Listener;

        for has_backup in [false, true] {
            let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let path = directory.path().join("settings.json");
            std::fs::write(&path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
            if has_backup {
                std::fs::write(path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
            }
            let startup_notice = settings_notice(&path, &settings::load(&path));
            let mut library = crate::library::Library::default();
            library.settings_notice = startup_notice.clone();
            let app = tauri::test::mock_builder()
                .manage(SettingsFile::new(path.clone()))
                .manage(Mutex::new(library))
                .invoke_handler(tauri::generate_handler![set_picker_preferences])
                .build(tauri::generate_context!(test = true))
                .unwrap_or_else(|e| panic!("cannot build preference fixture: {e}"));
            let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
                .build()
                .unwrap_or_else(|e| panic!("cannot build window fixture: {e}"));
            let (sent, events) = mpsc::channel();
            app.listen("library:changed", move |event| {
                let _ = sent.send(event.payload().to_string());
            });
            // Even unchanged preferences complete recovery.
            let chosen = PickerPreferences::from_settings(&settings::load(&path).settings);
            let result = tauri::test::get_ipc_response(&window, preference_request(chosen, None));
            assert_eq!(result.is_ok(), has_backup);
            let library = app.state::<Mutex<crate::library::Library>>();
            let library = library.lock().unwrap_or_else(|_| panic!("library lock poisoned"));
            if has_backup {
                assert_eq!(
                    library.settings_notice, None,
                    "successful recovery must clear the damaged-settings warning"
                );
                let event = events
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap_or_else(|e| panic!("{e}"));
                let status: serde_json::Value = serde_json::from_str(&event).unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(status.get("settings_notice"), Some(&serde_json::Value::Null));
            } else {
                assert_eq!(library.settings_notice, startup_notice);
                assert!(events.try_recv().is_err(), "failed saves must keep the warning");
            }
        }
    }

    #[test]
    fn preference_ipc_rejects_invalid_settings_without_private_details_or_overwriting() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("settings.json");
        let app = tauri::test::mock_builder()
            .manage(SettingsFile::new(path.clone()))
            .invoke_handler(tauri::generate_handler![set_picker_preferences])
            .build(tauri::generate_context!(test = true))
            .unwrap_or_else(|e| panic!("cannot build preference fixture: {e}"));
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap_or_else(|e| panic!("cannot build window fixture: {e}"));
        let chosen = PickerPreferences::from_settings(&settings::Settings::default());
        for (text, expected) in [
            (
                r#"{"copy_format":"synthetic-private-value"}"#,
                "settings.json is not valid",
            ),
            (
                r#"{"schema_version":3,"private":"synthetic-private-value"}"#,
                "settings.json was written by a newer version of Ibis",
            ),
        ] {
            std::fs::write(&path, text).unwrap_or_else(|e| panic!("{e}"));
            let result = tauri::test::get_ipc_response(&window, preference_request(chosen, None));
            assert_eq!(result.err(), Some(serde_json::json!(expected)));
            assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), text);
        }
    }

    #[test]
    #[cfg(unix)]
    fn dangling_primary_notice_keeps_the_save_refusal_warning() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let target = directory.path().join("synthetic-private-target");
        std::os::unix::fs::symlink(&target, &file.path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(file.path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
        let loaded = settings::load(&file.path);
        let notice = settings_notice(&file.path, &loaded).unwrap_or_else(|| panic!("expected a recovery notice"));
        assert!(notice.contains("settings.json could not be read; using the backup"));
        assert!(notice.contains("Changes are not being saved while this problem remains."));
        assert!(!notice.contains("The next save"));
        assert!(!notice.contains("synthetic-private-target"));
        assert_eq!(
            file.save_preferences(PickerPreferences::from_settings(&loaded.settings), None)
                .err(),
            Some("settings.json could not be read".to_string())
        );
        assert_eq!(std::fs::read_link(&file.path).unwrap_or_else(|e| panic!("{e}")), target);
        assert!(!target.exists());
        assert_eq!(
            std::fs::read(file.path.with_extension("json.bak")).unwrap_or_default(),
            b"{}"
        );
    }

    #[test]
    fn missing_primary_notices_and_failed_saves_name_the_refused_backup() {
        for (contents, expected) in [
            (
                br#"{"schema_version":99,"private":"synthetic-private-value"}"#.as_slice(),
                "settings.json.bak was written by a newer version of Ibis",
            ),
            (
                br#"{"copy_format":"synthetic-private-value"}"#.as_slice(),
                "settings.json.bak is not valid",
            ),
            (b"\xff".as_slice(), "settings.json.bak could not be read"),
            (
                br#"{"library":{"zotero":["user",{"group":"synthetic-private-value"}]}}"#.as_slice(),
                "settings.json.bak is not valid: Zotero library entry 2 must be \"user\" or {\"group\": <non-negative integer ID>}",
            ),
        ] {
            let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
            let file = SettingsFile::new(directory.path().join("settings.json"));
            let backup = file.path.with_extension("json.bak");
            std::fs::write(&backup, contents).unwrap_or_else(|e| panic!("{e}"));
            let loaded = settings::load(&file.path);
            let notice = settings_notice(&file.path, &loaded).unwrap_or_else(|| panic!("expected a warning"));
            assert!(notice.starts_with(expected), "{notice}");
            assert!(notice.contains("using defaults"));
            assert!(notice.contains("Changes are not being saved"));
            assert!(!notice.contains("synthetic-private-value"));
            assert!(notice.ends_with(backup.to_string_lossy().as_ref()));
            for _ in 0..2 {
                assert_eq!(
                    file.save_preferences(
                        PickerPreferences::from_settings(&loaded.settings),
                        Some(KeyFormat::Keys)
                    )
                    .err(),
                    Some(expected.to_string())
                );
            }
            assert!(!file.path.exists());
            assert_eq!(std::fs::read(&backup).unwrap_or_else(|e| panic!("{e}")), contents);
        }
    }

    #[test]
    fn missing_primary_notice_explains_restoration_and_clears_after_a_successful_save() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let backup = br#"{"copy_format":"typst","future_choice":[true,null]}"#;
        std::fs::write(file.path.with_extension("json.bak"), backup).unwrap_or_else(|e| panic!("{e}"));
        let loaded = settings::load(&file.path);
        let notice = settings_notice(&file.path, &loaded).unwrap_or_else(|| panic!("expected a recovery notice"));
        assert!(notice.contains("settings.json is missing; using the backup"));
        assert!(notice.contains("The next save will restore settings.json and keep the backup unchanged."));
        assert!(!notice.contains("not being saved"));
        assert!(!notice.contains("invalid-N"));
        let saved = file
            .save_preferences(PickerPreferences::from_settings(&loaded.settings), None)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved, loaded.settings);
        let restarted = settings::load(&file.path);
        assert_eq!(restarted.settings, saved);
        assert!(settings_notice(&file.path, &restarted).is_none());
        assert_eq!(
            std::fs::read(file.path.with_extension("json.bak")).unwrap_or_default(),
            backup
        );
    }

    #[test]
    fn unreadable_settings_report_no_operating_system_payload() {
        let loaded = settings::Loaded {
            settings: settings::Settings::default(),
            problem: Some(settings::SettingsError::Unreadable(std::io::Error::other(
                "synthetic-private-detail",
            ))),
            primary_read: settings::PrimaryRead::Unreadable,
            from_backup: false,
        };
        let notice = settings_notice(Path::new("synthetic/settings.json"), &loaded)
            .unwrap_or_else(|| panic!("expected a settings recovery notice"));
        assert!(notice.contains("could not be read"));
        assert!(!notice.contains("synthetic-private-detail"));
    }

    #[test]
    fn invalid_and_newer_settings_report_the_recovery_and_file_without_raw_error_payloads() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("settings.json");
        let private = "synthetic-private-payload";
        for (contents, problem) in [
            (format!(r#"{{"copy_format":"{private}"}}"#), "not valid"),
            (r#"{"schema_version":3}"#.to_string(), "newer version"),
            (
                format!(r#"{{"schema_version":2,"library":{{"zotero":["user",{{"group":"{private}"}}]}}}}"#),
                "Zotero library entry 2 must be \"user\" or {\"group\": <non-negative integer ID>}",
            ),
        ] {
            std::fs::write(&path, &contents).unwrap_or_else(|e| panic!("{e}"));
            let loaded = settings::load(&path);
            let notice =
                settings_notice(&path, &loaded).unwrap_or_else(|| panic!("expected a settings recovery notice"));
            assert!(notice.contains(problem));
            assert!(notice.contains("defaults"));
            assert!(notice.contains("not being saved"));
            assert!(notice.contains(path.to_string_lossy().as_ref()));
            assert!(!notice.contains(private));
            let Err(failed) = SettingsFile::new(path.clone()).update(|s| s.copy_format = Some(KeyFormat::Keys)) else {
                panic!("expected the invalid settings save to fail");
            };
            assert!(failed.contains(problem));
            assert!(!failed.contains(private));
            assert_eq!(
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}")),
                contents
            );
            std::fs::write(path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
            let recovered = settings::load(&path);
            let notice =
                settings_notice(&path, &recovered).unwrap_or_else(|| panic!("expected a settings recovery notice"));
            assert!(notice.contains("backup"));
            assert!(notice.contains(problem));
            assert!(!notice.contains(private));
            assert_eq!(
                notice.contains("not being saved"),
                contents.contains("schema_version\":3")
            );
            assert_eq!(
                notice.contains("settings.json.invalid-N"),
                !contents.contains("schema_version\":3")
            );
            std::fs::remove_file(path.with_extension("json.bak")).unwrap_or_else(|e| panic!("{e}"));
        }
        std::fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(settings_notice(&path, &settings::load(&path)).is_none());
    }

    #[test]
    fn settings_recovery_and_preference_changes_preserve_damaged_files_and_refuse_newer_files() {
        for contents in [
            b"".as_slice(),
            b"{invalid synthetic settings",
            b"\xff",
            br#"{"schema_version":999}"#,
        ] {
            for has_backup in [false, true] {
                let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
                let file = SettingsFile::new(directory.path().join("settings.json"));
                let backup = file.path.with_extension("json.bak");
                let temporary = file.path.with_extension("json.tmp");
                let backup_contents = br#"{"schema_version":2,"copy_format":"typst"}"#;
                std::fs::write(&file.path, contents).unwrap_or_else(|e| panic!("{e}"));
                std::fs::write(&temporary, b"synthetic unfinished save").unwrap_or_else(|e| panic!("{e}"));
                if has_backup {
                    std::fs::write(&backup, backup_contents).unwrap_or_else(|e| panic!("{e}"));
                }
                let loaded = settings::load(&file.path);
                assert!(loaded.problem.is_some());
                assert_eq!(loaded.from_backup, has_backup);
                assert_eq!(loaded.settings.copy_format, has_backup.then_some(KeyFormat::Typst));
                assert_eq!(std::fs::read(&file.path).unwrap_or_else(|e| panic!("{e}")), contents);
                let chosen = PickerPreferences::from_settings(&loaded.settings);
                let saved = file.save_preferences(chosen, Some(KeyFormat::Cite));
                let recoverable = has_backup && contents != br#"{"schema_version":999}"#;
                if recoverable {
                    let saved = saved.unwrap_or_else(|e| panic!("recovery save failed: {e}"));
                    assert_eq!(saved.copy_format, Some(KeyFormat::Cite));
                    let restarted = settings::load(&file.path);
                    assert!(restarted.problem.is_none());
                    assert_eq!(restarted.settings, saved);
                    assert_eq!(
                        std::fs::read(file.path.with_extension("json.invalid-1")).unwrap_or_else(|e| panic!("{e}")),
                        contents
                    );
                    assert_eq!(
                        std::fs::read(&backup).unwrap_or_else(|e| panic!("{e}")),
                        backup_contents
                    );
                    assert_eq!(
                        std::fs::read(&temporary).unwrap_or_default(),
                        b"synthetic unfinished save"
                    );
                    continue;
                }
                assert!(saved.is_err());
                assert!(!file.path.with_extension("json.invalid-1").exists());
                assert_eq!(std::fs::read(&file.path).unwrap_or_else(|e| panic!("{e}")), contents);
                if has_backup {
                    assert_eq!(
                        std::fs::read(&backup).unwrap_or_else(|e| panic!("{e}")),
                        backup_contents
                    );
                } else {
                    assert!(!backup.exists());
                }
                assert_eq!(
                    std::fs::read(&temporary).unwrap_or_else(|e| panic!("{e}")),
                    b"synthetic unfinished save"
                );
            }
        }
    }

    #[test]
    fn settings_saves_from_several_threads_never_lose_each_other() {
        let dir = std::env::temp_dir().join(format!("ibis-concurrent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = SettingsFile::new(dir.join("settings.json"));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(&file.path, "{ damaged").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(file.path.with_extension("json.bak"), "{}").unwrap_or_else(|e| panic!("{e}"));
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let file = file.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        let saved = if i % 2 == 0 {
                            file.update(|s| s.library = Some(LibrarySetting::Bibtex(PathBuf::from("refs.bib"))))
                        } else {
                            file.update(|s| s.copy_format = Some(KeyFormat::Typst))
                        };
                        saved.unwrap_or_else(|e| panic!("{e}"));
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap_or_else(|_| panic!("a saving thread panicked"));
        }
        let loaded = settings::load(&file.path);
        assert!(loaded.problem.is_none());
        assert_eq!(
            loaded.settings.library,
            Some(LibrarySetting::Bibtex(PathBuf::from("refs.bib")))
        );
        assert_eq!(loaded.settings.copy_format, Some(KeyFormat::Typst));
        assert_eq!(
            std::fs::read_to_string(file.path.with_extension("json.invalid-1")).unwrap_or_default(),
            "{ damaged"
        );
        assert!(!file.path.with_extension("json.invalid-2").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picker_preferences_save_only_real_choices_and_keep_other_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        file.update(|s| s.copy_format = Some(KeyFormat::Typst))
            .unwrap_or_else(|e| panic!("{e}"));
        let chosen = PickerPreferences {
            citation_style: Some(CitationStyle::Apa),
            density: Density::Spacious,
            larger_text: true,
            show_paste_preview: false,
            search_mark: false,
            paste: false,
        };
        let saved = file.save_preferences(chosen, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.paste, Some(false));
        assert_eq!(saved.copy_format, Some(KeyFormat::Typst));
        assert_eq!(PickerPreferences::from_settings(&saved), chosen);
        let defaults = PickerPreferences {
            citation_style: None,
            density: Density::Comfortable,
            larger_text: false,
            show_paste_preview: true,
            search_mark: false,
            paste: true,
        };
        let saved = file.save_preferences(defaults, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(PickerPreferences::from_settings(&saved), defaults);
        let text = std::fs::read_to_string(&file.path).unwrap_or_default();
        for field in [
            "citation_style",
            "density",
            "larger_text",
            "show_paste_preview",
            "paste",
        ] {
            assert!(!text.contains(field), "{field} is not written for the default");
        }
    }

    #[test]
    fn paste_preview_defaults_on_and_off_survives_restart_without_losing_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        assert!(PickerPreferences::from_settings(&settings::Settings::default()).show_paste_preview);

        let original = serde_json::json!({
            "schema_version": 2, "shortcut": "Alt+Space", "library": { "bibtex": "refs.bib" },
            "copy_format": "typst", "update_checks": false, "citation_style": "ieee",
            "density": "compact", "larger_text": true, "future_choice": { "kept": [true, null] },
        });
        std::fs::write(&file.path, original.to_string()).unwrap_or_else(|e| panic!("{e}"));
        let mut chosen = PickerPreferences::from_settings(&settings::load(&file.path).settings);
        chosen.show_paste_preview = false;
        file.save_preferences(chosen, None).unwrap_or_else(|e| panic!("{e}"));
        let restarted = SettingsFile::new(file.path.clone());
        let loaded = settings::load(&restarted.path);
        assert!(loaded.problem.is_none());
        assert_eq!(PickerPreferences::from_settings(&loaded.settings), chosen);
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file.path).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
        for (field, value) in original.as_object().unwrap_or_else(|| panic!("expected an object")) {
            assert_eq!(&saved[field], value, "{field} survives the preview choice");
        }
        assert_eq!(saved["show_paste_preview"], false);

        chosen.show_paste_preview = true;
        restarted
            .save_preferences(chosen, None)
            .unwrap_or_else(|e| panic!("{e}"));
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file.path).unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            saved.get("show_paste_preview").is_none(),
            "the on default stays unwritten"
        );
        assert_eq!(saved["future_choice"], original["future_choice"]);
    }

    #[test]
    fn key_choices_are_saved_with_style_removal_before_any_copy() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let styled = PickerPreferences {
            citation_style: Some(CitationStyle::Apa),
            density: Density::Compact,
            larger_text: true,
            show_paste_preview: false,
            search_mark: false,
            paste: true,
        };
        for format in [
            KeyFormat::Pandoc,
            KeyFormat::Autocite,
            KeyFormat::Parencite,
            KeyFormat::Textcite,
            KeyFormat::Cite,
            KeyFormat::Typst,
            KeyFormat::Keys,
        ] {
            file.save_preferences(styled, None).unwrap_or_else(|e| panic!("{e}"));
            let chosen = PickerPreferences {
                citation_style: None,
                ..styled
            };
            file.save_preferences(chosen, Some(format))
                .unwrap_or_else(|e| panic!("{e}"));
            let loaded = settings::load(&file.path);
            assert_eq!(loaded.settings.copy_format, Some(format));
            assert_eq!(PickerPreferences::from_settings(&loaded.settings), chosen);
        }
        // A style/density save keeps a key changed in the file meanwhile.
        file.update(|s| s.copy_format = Some(KeyFormat::Keys))
            .unwrap_or_else(|e| panic!("{e}"));
        let saved = file.save_preferences(styled, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.copy_format, Some(KeyFormat::Keys));
        assert_eq!(saved.citation_style, Some(CitationStyle::Apa));
    }

    #[test]
    fn choosing_a_key_never_overwrites_invalid_newer_or_unreadable_settings() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let file = SettingsFile::new(directory.path().join("settings.json"));
        let chosen = PickerPreferences {
            citation_style: None,
            density: Density::Comfortable,
            larger_text: false,
            show_paste_preview: false,
            search_mark: false,
            paste: true,
        };
        for text in [
            r#"{"copy_format":"synthetic-private-value"}"#,
            r#"{"show_paste_preview":"synthetic-private-value"}"#,
            r#"{"schema_version":3,"show_paste_preview":false,"future_choice":{"kept":true}}"#,
        ] {
            std::fs::write(&file.path, text).unwrap_or_else(|e| panic!("{e}"));
            let failed = file.save_preferences(chosen, Some(KeyFormat::Cite));
            assert!(failed.is_err());
            assert!(
                !failed
                    .err()
                    .unwrap_or_else(|| panic!("expected a failed settings save"))
                    .contains("synthetic-private-value")
            );
            assert_eq!(std::fs::read_to_string(&file.path).unwrap_or_default(), text);
        }
        std::fs::remove_file(&file.path).unwrap_or_else(|e| panic!("{e}"));
        std::fs::create_dir(&file.path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            file.save_preferences(chosen, Some(KeyFormat::Cite))
                .err()
                .unwrap_or_else(|| panic!("expected an unreadable settings save to fail")),
            "settings.json could not be read"
        );
        assert!(file.path.is_dir());
    }
}
