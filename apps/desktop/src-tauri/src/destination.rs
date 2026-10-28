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
    /// What the app's documents use; `None` follows the person's default.
    pub(crate) automatic: Option<AppOutput>,
    /// The output the person chose to always use for this app.
    pub(crate) remembered: Option<AppOutput>,
    /// The per-app file could not be read, so defaults apply.
    pub(crate) unreadable: bool,
}

/// The output an app's documents use, from the destination table through
/// `output_for`, as `ibis cite --for-app` gets it. Formatted apps get the
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
    let mut text = String::new();
    fs::File::open(path)?.take(MAX_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
    }
    let file: MemoryFile = serde_json::from_str(&text).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if file.version != VERSION || file.apps.len() > MAX_APPS {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
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
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    let partial = path.with_extension("json.tmp");
    let mut file = fs::File::create(&partial)?;
    file.write_all(format!("{json}\n").as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&partial, path)?;
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
            automatic: None,
            remembered: None,
            unreadable: memory.unreadable(),
        };
    };
    let style = settings::load(&settings_file.path).settings.citation_style;
    let (remembered, unreadable) = memory.get(&id);
    PickerDestination {
        app: Some(name),
        automatic: automatic(&id, style),
        remembered,
        unreadable,
    }
}

/// Always use `output` for the app the picker was opened from; `None` returns it to Automatic.
#[tauri::command]
#[specta::specta]
pub(crate) fn remember_app_output(
    app: AppHandle,
    memory: State<'_, AppMemory>,
    output: Option<AppOutput>,
) -> Result<(), String> {
    let (id, _) = crate::paste::last_app(&app).ok_or("no app to remember a choice for")?;
    memory
        .set(&id, output)
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
}
