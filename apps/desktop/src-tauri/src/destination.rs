//! The output the picker starts with for the app it was opened from (#1202),
//! and the per-app choices Ibis remembers ("What Ibis remembers per app" in
//! docs/decisions.md).
//!
//! `apps.json`, beside `settings.json`, holds at most [`MAX_APPS`] app IDs,
//! each with the output chosen for it. Never a window title, document name or
//! citation text. A file Ibis cannot read means defaults, and the picker says
//! so; the next choice replaces it.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ibis_core::cite::KeyFormat;
use ibis_core::contract::citation::{KeySyntax, OutputSpec};
use ibis_core::destination::{AppId, lookup, output_for};
use ibis_core::settings::{self, CitationStyle};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::preferences::SettingsFile;

/// Apps remembered; the least recently chosen goes first.
const MAX_APPS: usize = 200;
/// A file this large was not written by Ibis.
const MAX_BYTES: u64 = 256 * 1024;
const VERSION: u32 = 1;

/// One output the picker can copy: a key syntax or a citation style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum AppOutput {
    Key(KeyFormat),
    Style(CitationStyle),
}

/// What the picker needs to know about the app it was opened from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct PickerDestination {
    /// The app's own name; `None` when no app was captured.
    pub(crate) app: Option<String>,
    /// The captured OS app identifier, used to refuse a save after the target changes.
    pub(crate) app_id: Option<String>,
    /// The app suggestion; `None` follows the saved global default.
    pub(crate) automatic: Option<AppOutput>,
    /// The output the person chose to always use for this app.
    pub(crate) remembered: Option<AppOutput>,
    /// The per-app file could not be read, so defaults apply.
    pub(crate) unreadable: bool,
    /// The current paste policy permits an attempt; final delivery checks still apply.
    pub(crate) can_paste: bool,
}

/// The output an app's documents use, from the destination table through
/// `output_for`. Formatted apps get the
/// default style, APA when the person has none; apps the table gives no
/// output follow the person's default (`None`).
pub(crate) fn automatic(app: &AppId, default_style: Option<CitationStyle>) -> Option<AppOutput> {
    lookup(app)?.output?;
    let style = default_style.unwrap_or(CitationStyle::Apa);
    let id = serde_json::to_value(style).ok()?.as_str()?.to_owned();
    match output_for(Some(app), &OutputSpec::Csl { style: id.clone() }, &id) {
        OutputSpec::Csl { .. } => Some(AppOutput::Style(style)),
        OutputSpec::Key { syntax, .. } => match syntax {
            KeySyntax::Pandoc => Some(AppOutput::Key(KeyFormat::Pandoc)),
            KeySyntax::Biblatex => Some(AppOutput::Key(KeyFormat::Autocite)),
            KeySyntax::Latex | KeySyntax::Natbib => Some(AppOutput::Key(KeyFormat::Cite)),
            KeySyntax::Typst => Some(AppOutput::Key(KeyFormat::Typst)),
            _ => None,
        },
        OutputSpec::Preset { .. } => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Remembered {
    app: AppId,
    output: AppOutput,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryFile {
    version: u32,
    apps: Vec<Remembered>,
}

/// The per-app file, read once and written on every change.
pub(crate) struct AppMemory {
    path: PathBuf,
    state: Mutex<(Vec<Remembered>, bool)>,
}

fn normalized(app: &AppId) -> AppId {
    AppId {
        system: app.system,
        id: app.normalized(),
    }
}

impl AppMemory {
    /// Read `path`; a missing file is empty, anything else unreadable is
    /// reported and treated as empty.
    pub(crate) fn open(path: PathBuf) -> Self {
        let (apps, unreadable) = match read(&path) {
            Ok(apps) => (apps, false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (Vec::new(), false),
            Err(_) => (Vec::new(), true),
        };
        AppMemory {
            path,
            state: Mutex::new((apps, unreadable)),
        }
    }

    /// The remembered output for `app`, and whether the file was unreadable.
    pub(crate) fn get(&self, app: &AppId) -> (Option<AppOutput>, bool) {
        let app = normalized(app);
        self.state.lock().map_or((None, true), |state| {
            let output = state.0.iter().find(|entry| entry.app == app).map(|entry| entry.output);
            (output, state.1)
        })
    }

    fn unreadable(&self) -> bool {
        self.state.lock().map_or(true, |state| state.1)
    }

    /// Remember `output` for `app`, or forget the app with `None`.
    pub(crate) fn set(&self, app: &AppId, output: Option<AppOutput>) -> io::Result<()> {
        let app = normalized(app);
        self.change(|apps| {
            apps.retain(|entry| entry.app != app);
            if let Some(output) = output {
                apps.push(Remembered { app, output });
            }
        })
    }

    /// Forget every app.
    pub(crate) fn forget(&self) -> io::Result<()> {
        self.change(Vec::clear)
    }

    fn change(&self, change: impl FnOnce(&mut Vec<Remembered>)) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("app choices are unavailable"))?;
        let mut apps = state.0.clone();
        change(&mut apps);
        let excess = apps.len().saturating_sub(MAX_APPS);
        apps.drain(..excess);
        write(&self.path, &apps)?;
        *state = (apps, false);
        Ok(())
    }
}

fn read(path: &Path) -> io::Result<Vec<Remembered>> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut text = String::new();
    fs::File::open(path)?.take(MAX_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
    }
    let mut file: MemoryFile = serde_json::from_str(&text).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if file.version != VERSION || file.apps.len() > MAX_APPS {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut ids = std::collections::HashSet::new();
    for entry in &mut file.apps {
        entry.app = normalized(&entry.app);
        if entry.app.id.is_empty() || entry.app.id.chars().any(char::is_control) || !ids.insert(entry.app.clone()) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
    }
    Ok(file.apps)
}

/// Write the whole file beside the old one, then swap it in, so a crash
/// leaves either file whole.
fn write(path: &Path, apps: &[Remembered]) -> io::Result<()> {
    let json = serde_json::to_string_pretty(&MemoryFile {
        version: VERSION,
        apps: apps.to_vec(),
    })?;
    if json.len() as u64 + 1 > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
    }
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    // Exclusive creation never follows a leftover temporary symlink or truncates it.
    let (partial, mut file) = (1..=1000)
        .find_map(|n| {
            let partial = path.with_extension(format!("json.tmp-{n}"));
            match fs::OpenOptions::new().write(true).create_new(true).open(&partial) {
                Ok(file) => Some(Ok((partial, file))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .unwrap_or_else(|| {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "temporary names exhausted",
            ))
        })?;
    let written = file
        .write_all(format!("{json}\n").as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let replaced = written.and_then(|()| fs::rename(&partial, path));
    if replaced.is_err() {
        let _ = fs::remove_file(&partial);
    }
    replaced?;
    #[cfg(unix)]
    if let Some(directory) = path.parent() {
        let _ = fs::File::open(directory).and_then(|d| d.sync_all());
    }
    Ok(())
}

/// The per-app file beside the settings file.
pub(crate) fn memory_path(settings: &Path) -> PathBuf {
    settings.with_file_name("apps.json")
}

/// The app the picker was opened from, its output and what Ibis remembers for it.
#[tauri::command]
#[specta::specta]
pub(crate) fn picker_destination(
    app: AppHandle,
    memory: State<'_, AppMemory>,
    settings_file: State<'_, SettingsFile>,
) -> PickerDestination {
    let Some((id, name)) = crate::paste::last_app(&app) else {
        return PickerDestination {
            app: None,
            app_id: None,
            automatic: None,
            remembered: None,
            unreadable: memory.unreadable(),
            can_paste: false,
        };
    };
    let settings = settings::load(&settings_file.path).settings;
    let can_paste = crate::paste::intends_paste(&app, settings.paste.unwrap_or(true));
    let style = settings.citation_style;
    let (remembered, unreadable) = memory.get(&id);
    PickerDestination {
        app: Some(name),
        app_id: Some(id.normalized()),
        automatic: automatic(&id, style),
        remembered,
        unreadable,
        can_paste,
    }
}

/// Always use `output` for the app the picker was opened from; `None` returns it to Automatic.
#[tauri::command]
#[specta::specta]
pub(crate) fn remember_app_output(
    app: AppHandle,
    memory: State<'_, AppMemory>,
    app_id: String,
    output: Option<AppOutput>,
) -> Result<(), String> {
    let (id, _) = crate::paste::last_app(&app).ok_or("no app to remember a choice for")?;
    remember_for(&memory, &id, &app_id, output)
}

fn remember_for(memory: &AppMemory, id: &AppId, expected: &str, output: Option<AppOutput>) -> Result<(), String> {
    if id.normalized() != expected {
        return Err("the app changed; open the output menu again".into());
    }
    memory
        .set(id, output)
        .map_err(|_| "the choice for this app could not be saved".into())
}

/// Forget the outputs chosen for every app.
#[tauri::command]
#[specta::specta]
pub(crate) fn forget_app_outputs(memory: State<'_, AppMemory>) -> Result<(), String> {
    memory
        .forget()
        .map_err(|_| "the choices for apps could not be forgotten".into())
}

/// Read the per-app file once the settings location is known.
pub(crate) fn manage(app: &AppHandle, settings: &Path) {
    app.manage(AppMemory::open(memory_path(settings)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::System;

    fn mac(id: &str) -> AppId {
        AppId {
            system: System::Macos,
            id: id.into(),
        }
    }

    #[test]
    fn each_kind_of_app_gets_its_output_and_unknown_apps_follow_the_default() {
        let style = |style| Some(AppOutput::Style(style));
        let key = |format| Some(AppOutput::Key(format));
        assert_eq!(automatic(&mac("com.microsoft.Word"), None), style(CitationStyle::Apa));
        assert_eq!(
            automatic(&mac("com.microsoft.Word"), Some(CitationStyle::Ieee)),
            style(CitationStyle::Ieee)
        );
        assert_eq!(automatic(&mac("md.obsidian"), None), key(KeyFormat::Pandoc));
        assert_eq!(automatic(&mac("texshop"), None), key(KeyFormat::Autocite));
        let windows = AppId {
            system: System::Windows,
            id: r"C:\Program Files\TeXstudio\texstudio.exe".into(),
        };
        assert_eq!(automatic(&windows, None), key(KeyFormat::Autocite));
        assert_eq!(automatic(&mac("com.apple.Terminal"), None), None);
        assert_eq!(automatic(&mac("org.example.unknown"), None), None);
    }

    #[test]
    fn a_remembered_choice_survives_a_restart_and_forgetting_clears_it() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = memory_path(&directory.path().join("settings.json"));
        let memory = AppMemory::open(path.clone());
        assert_eq!(memory.get(&mac("com.microsoft.Word")), (None, false));
        let apa = AppOutput::Style(CitationStyle::Apa);
        memory
            .set(&mac("com.microsoft.Word"), Some(apa))
            .unwrap_or_else(|e| panic!("{e}"));
        let reopened = AppMemory::open(path.clone());
        // IDs compare as the platform does: bundle IDs without case.
        assert_eq!(reopened.get(&mac("com.microsoft.word")), (Some(apa), false));
        reopened
            .set(&mac("com.microsoft.word"), None)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            AppMemory::open(path.clone()).get(&mac("com.microsoft.Word")),
            (None, false)
        );
        reopened
            .set(&mac("md.obsidian"), Some(AppOutput::Key(KeyFormat::Typst)))
            .unwrap_or_else(|e| panic!("{e}"));
        reopened.forget().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(AppMemory::open(path).get(&mac("md.obsidian")), (None, false));
    }

    #[test]
    fn the_file_holds_only_app_ids_and_choices_and_keeps_the_latest_apps() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = memory_path(&directory.path().join("settings.json"));
        let memory = AppMemory::open(path.clone());
        for n in 0..=MAX_APPS {
            memory
                .set(
                    &mac(&format!("org.example.app{n}")),
                    Some(AppOutput::Key(KeyFormat::Pandoc)),
                )
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let reopened = AppMemory::open(path.clone());
        assert_eq!(
            reopened.get(&mac("org.example.app0")).0,
            None,
            "the oldest app is dropped"
        );
        assert!(reopened.get(&mac(&format!("org.example.app{MAX_APPS}"))).0.is_some());
        let file: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap_or_default()).unwrap_or_default();
        assert_eq!(file["apps"].as_array().map(Vec::len), Some(MAX_APPS));
        assert_eq!(
            file["apps"][0],
            serde_json::json!({ "app": { "system": "macos", "id": "org.example.app1" }, "output": { "key": "pandoc" } })
        );
    }

    #[test]
    fn an_unreadable_file_means_defaults_with_a_notice_until_the_next_choice() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = memory_path(&directory.path().join("settings.json"));
        for damaged in [
            "not json".to_string(),
            r#"{"version":2,"apps":[]}"#.to_string(),
            r#"{"version":1,"apps":[],"titles":["Draft"]}"#.to_string(),
            r#"{"version":1,"apps":[{"app":{"system":"macos","id":"example.browser"},"output":{"key":"cite"}},{"app":{"system":"macos","id":"EXAMPLE.BROWSER"},"output":{"key":"typst"}}]}"#.to_string(),
            format!(
                r#"{{"version":1,"apps":[],"pad":"{}"}}"#,
                "x".repeat(MAX_BYTES as usize)
            ),
        ] {
            fs::write(&path, damaged).unwrap_or_else(|e| panic!("{e}"));
            let memory = AppMemory::open(path.clone());
            assert_eq!(memory.get(&mac("com.microsoft.Word")), (None, true));
            memory
                .set(&mac("com.microsoft.Word"), Some(AppOutput::Key(KeyFormat::Cite)))
                .unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(
                AppMemory::open(path.clone()).get(&mac("com.microsoft.Word")),
                (Some(AppOutput::Key(KeyFormat::Cite)), false)
            );
        }
    }
    #[test]
    fn ambiguous_browser_and_editor_ids_have_no_inferred_document_syntax() {
        for id in ["com.google.Chrome", "com.apple.Safari", "com.microsoft.VSCode"] {
            assert_eq!(automatic(&mac(id), None), None);
        }
        for id in ["chrome.exe", "msedge.exe", "code.exe"] {
            assert_eq!(
                automatic(
                    &AppId {
                        system: System::Windows,
                        id: id.into()
                    },
                    None
                ),
                None
            );
        }
    }

    #[test]
    fn a_stale_app_cannot_receive_the_previous_apps_choice() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let memory = AppMemory::open(directory.path().join("apps.json"));
        assert!(
            remember_for(
                &memory,
                &mac("example.editor"),
                "example.browser",
                Some(AppOutput::Key(KeyFormat::Cite))
            )
            .is_err()
        );
        assert_eq!(memory.get(&mac("example.editor")), (None, false));
    }

    #[test]
    fn a_failed_or_oversized_save_keeps_the_previous_durable_choice() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("apps.json");
        let memory = AppMemory::open(path.clone());
        let cite = AppOutput::Key(KeyFormat::Cite);
        memory
            .set(&mac("example.browser"), Some(cite))
            .unwrap_or_else(|e| panic!("{e}"));
        let before = fs::read(&path).unwrap_or_default();
        assert!(memory.set(&mac(&"x".repeat(MAX_BYTES as usize)), Some(cite)).is_err());
        assert_eq!(fs::read(&path).unwrap_or_default(), before);
        assert_eq!(memory.get(&mac("example.browser")), (Some(cite), false));
        // A directory at the destination refuses the atomic replacement.
        fs::remove_file(&path).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(memory.forget().is_err());
        assert_eq!(memory.get(&mac("example.browser")), (Some(cite), false));
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_temporary_symlink_is_never_truncated() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("apps.json");
        let occupied = directory.path().join("occupied");
        fs::write(&occupied, "synthetic existing data").unwrap_or_else(|e| panic!("{e}"));
        std::os::unix::fs::symlink(&occupied, path.with_extension("json.tmp-1")).unwrap_or_else(|e| panic!("{e}"));
        AppMemory::open(path)
            .set(&mac("example.browser"), Some(AppOutput::Key(KeyFormat::Cite)))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            fs::read_to_string(occupied).unwrap_or_default(),
            "synthetic existing data"
        );
    }
    #[cfg(unix)]
    #[test]
    fn nonregular_memory_files_use_defaults_without_opening_the_stream() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = directory.path().join("apps.json");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(AppMemory::open(path).get(&mac("example.browser")), (None, true));
    }
}

#[cfg(test)]
mod acceptance;
