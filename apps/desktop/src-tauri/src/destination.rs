//! The output the picker starts with for the app it was opened from (#1202),
//! and the per-app choices Ibis remembers (decision 0031,
//! docs/decisions/0031-what-ibis-remembers-per-app.md).
//!
//! `apps.json`, beside `settings.json`, holds at most [`MAX_APPS`] app IDs,
//! each with its chosen output and optional paste/copy-only preference.
//! Never a window title, document name or
//! citation text. An unreadable file uses default formats and retains a durable
//! delivery-recovery refusal until each app gets an explicit delivery choice.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ibis_core::contract::citation::OutputSpec;
use ibis_core::destination::{
    AppId, AppPaste, CopyReason, OutputQuestion, PasteSafety, has_citation_plugins, lookup, output_for,
};
use ibis_core::settings;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::output::OutputRef;
use crate::preferences::SettingsFile;

/// Apps remembered; the least recently chosen goes first.
const MAX_APPS: usize = 200;
/// A file this large was not written by Ibis.
const MAX_BYTES: u64 = 256 * 1024;
const VERSION: u32 = 1;

/// What the picker needs to know about the app it was opened from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct PickerDestination {
    /// The app's own name; `None` when no app was captured.
    pub(crate) app: Option<String>,
    /// The captured OS app identifier, used to refuse a save after the target changes.
    pub(crate) app_id: Option<String>,
    /// Opaque app-location fingerprint; no path crosses IPC.
    pub(crate) app_identity: Option<String>,
    /// The app suggestion, an engine output id; `None` follows the saved default.
    #[specta(type = Option<String>)]
    pub(crate) automatic: Option<OutputRef>,
    /// The output the person chose to always use for this app.
    #[specta(type = Option<String>)]
    pub(crate) remembered: Option<OutputRef>,
    /// This app needs a delivery choice after an unreadable per-app file; formats use defaults.
    pub(crate) unreadable: bool,
    /// The current paste policy permits an attempt; final delivery checks still apply.
    pub(crate) can_paste: bool,
    /// The app cannot identify document syntax; ask until an output is remembered.
    pub(crate) output_question: Option<OutputQuestion>,
    /// The separate delivery choice; never inferred from a remembered format.
    pub(crate) paste: Option<AppPaste>,
    /// An unrecognised app has a proven target but no delivery choice yet.
    pub(crate) paste_question: bool,
    /// A proven ordinary target can accept an explicit delivery choice, including recovery.
    pub(crate) can_choose_paste: bool,
    #[specta(optional)]
    pub(crate) writing_project: Option<crate::writing_projects::WritingProjectView>,
    #[specta(optional)]
    pub(crate) context_version: Option<(u32, u32)>,
    #[specta(optional)]
    pub(crate) writing_context: Option<String>,
}

/// The output an app's documents use, from the destination table through
/// `output_for`, as the engine names it. Formatted apps get the default
/// output's style, else the engine's default style; apps the table gives no
/// output follow the person's default (`None`).
pub(crate) fn automatic(app: &AppId, default: OutputRef) -> Option<OutputRef> {
    lookup(app)?.output?;
    let style = crate::output::default_style(default);
    let output = output_for(
        Some(app),
        &OutputSpec::Csl {
            style: style.to_owned(),
        },
        style,
    );
    ibis_engine::outputs::named(&output).and_then(|entry| crate::output::listed(&entry.id))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Remembered {
    app: AppId,
    /// An engine output id; one this Ibis does not list reads as no choice.
    #[serde(default, deserialize_with = "crate::output::lenient")]
    output: Option<OutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paste: Option<AppPaste>,
    /// Only unlisted apps approved for Paste need a location binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<String>,
    /// The one-time note about reference manager plugins has been shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    noted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryFile {
    version: u32,
    apps: Vec<Remembered>,
    /// Lost delivery choices remain refused until deliberately resolved.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    recovery: bool,
}

/// The per-app file, read once and written on every change.
pub(crate) struct AppMemory {
    path: PathBuf,
    /// Every app's choices, and whether lost choices still need resolving.
    state: Mutex<(Vec<Remembered>, bool)>,
    /// Format choices an earlier Ibis wrote, which this one cannot read, are
    /// kept as `.earlier` copies (decision 0016): whether to say so this run,
    /// until a format is chosen, and whether `apps.json`'s copy could not be
    /// made, so it is never rewritten.
    earlier: Mutex<(bool, bool)>,
}

/// A copy beside a file Ibis keeps whole once, before anything rewrites it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Copy {
    /// Made now, completely: say so.
    Made,
    /// Made at an earlier start: already said.
    Kept,
    Failed,
}

/// Keep `path` whole as `name` beside it, once. The copy is written and
/// flushed under a temporary name, then linked into place, so a copy that
/// exists is complete, and an existing one is never replaced.
fn keep_copy(path: &Path, name: &str) -> Copy {
    let copy = path.with_file_name(name);
    match fs::symlink_metadata(&copy) {
        Ok(found) => return if found.is_file() { Copy::Kept } else { Copy::Failed },
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Copy::Failed,
        Err(_) => {}
    }
    let partial = path.with_file_name(format!("{name}.partial"));
    // Only an interrupted copy leaves this name behind.
    let _ = fs::remove_file(&partial);
    let made = fs::read(path).and_then(|bytes| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&partial)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::hard_link(&partial, &copy)
    });
    let _ = fs::remove_file(&partial);
    match made {
        Ok(()) => Copy::Made,
        Err(_) if fs::symlink_metadata(&copy).is_ok_and(|found| found.is_file()) => Copy::Kept,
        Err(_) => Copy::Failed,
    }
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
        let (apps, unreadable, parsed) = match read(&path) {
            Ok(file) => (file.apps, file.recovery, true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (Vec::new(), false, false),
            Err(_) => (Vec::new(), true, false),
        };
        // A file marked for delivery recovery still holds the earlier formats.
        let copy = (parsed && earlier_formats(&path)).then(|| keep_copy(&path, "apps.json.earlier"));
        AppMemory {
            path,
            state: Mutex::new((apps, unreadable)),
            earlier: Mutex::new((copy == Some(Copy::Made), copy == Some(Copy::Failed))),
        }
    }

    /// 0.2 kept its default format in `settings.json` keys this Ibis keeps in
    /// the file but does not read: kept as a copy and said once, like the apps'.
    pub(crate) fn note_earlier_settings(&self, settings: &Path) {
        let earlier = fs::read(settings)
            .ok()
            .and_then(|text| serde_json::from_slice::<serde_json::Value>(&text).ok())
            .is_some_and(|file| {
                ["copy_format", "citation_style"]
                    .iter()
                    .any(|key| file.get(*key).is_some())
            });
        if earlier
            && keep_copy(settings, "settings.json.earlier") != Copy::Kept
            && let Ok(mut state) = self.earlier.lock()
        {
            state.0 = true;
        }
    }

    /// Whether to say that earlier format choices could not be read.
    pub(crate) fn says_earlier_formats(&self) -> bool {
        self.earlier.lock().is_ok_and(|earlier| earlier.0)
    }

    /// A format was chosen, for an app or as the default: the notice is answered.
    pub(crate) fn answer_earlier_formats(&self) {
        if let Ok(mut earlier) = self.earlier.lock() {
            earlier.0 = false;
        }
    }

    /// The remembered output for `app`, and whether the file was unreadable.
    pub(crate) fn get(&self, app: &AppId) -> (Option<OutputRef>, bool) {
        let app = normalized(app);
        self.state.lock().map_or((None, true), |state| {
            let output = state
                .0
                .iter()
                .find(|entry| entry.app == app)
                .and_then(|entry| entry.output);
            (output, state.1)
        })
    }

    pub(crate) fn paste_for(&self, app: &AppId, identity: Option<&str>) -> Result<Option<AppPaste>, CopyReason> {
        let app = normalized(app);
        let state = self.state.lock().map_err(|_| CopyReason::PasteUnavailable)?;
        let entry = state.0.iter().find(|entry| entry.app == app);
        let paste = entry.and_then(|entry| entry.paste).filter(|paste| {
            *paste != AppPaste::Paste
                || lookup(&app).is_some()
                || identity.is_some() && entry.and_then(|entry| entry.identity.as_deref()) == identity
        });
        if state.1 && paste.is_none() {
            return Err(CopyReason::PasteUnavailable);
        }
        Ok(paste)
    }

    pub(crate) fn for_capture(&self, captured: &crate::paste::Captured) -> Result<Option<AppPaste>, CopyReason> {
        captured
            .target
            .app
            .as_ref()
            .map_or(Ok(None), |id| self.paste_for(id, captured.app_identity.as_deref()))
    }

    #[cfg(test)]
    pub(crate) fn paste(&self, app: &AppId) -> Result<Option<AppPaste>, CopyReason> {
        self.paste_for(app, test_identity(app).as_deref())
    }

    /// Remember `output` for `app`.
    #[cfg(test)]
    pub(crate) fn set(&self, app: &AppId, output: Option<OutputRef>) -> io::Result<()> {
        self.set_choice(app, output, None)
    }

    #[cfg(test)]
    pub(crate) fn set_choice(&self, app: &AppId, output: Option<OutputRef>, paste: Option<AppPaste>) -> io::Result<()> {
        self.set_bound_choice(app, output, paste, test_identity(app))
    }

    /// Remember what was chosen; the half left out keeps its saved value.
    fn set_bound_choice(
        &self,
        app: &AppId,
        output: Option<OutputRef>,
        paste: Option<AppPaste>,
        identity: Option<String>,
    ) -> io::Result<()> {
        let app = normalized(app);
        if output.is_none() && paste.is_none() {
            return Err(io::Error::other("there is no choice to remember"));
        }
        if paste == Some(AppPaste::Paste) && lookup(&app).is_some_and(|entry| entry.safety != PasteSafety::Normal) {
            return Err(io::Error::other("this app cannot be approved for automatic paste"));
        }
        if paste == Some(AppPaste::Paste) && lookup(&app).is_none() && identity.is_none() {
            return Err(io::Error::other("this app has no proven identity to approve"));
        }
        let chose_output = output.is_some();
        self.change(|apps| {
            let previous = apps.iter().find(|entry| entry.app == app);
            let output = output.or_else(|| previous?.output);
            let paste = paste.or_else(|| {
                previous?.paste.filter(|paste| {
                    *paste != AppPaste::Paste
                        || lookup(&app).is_some()
                        || identity.is_some() && previous.and_then(|entry| entry.identity.as_ref()) == identity.as_ref()
                })
            });
            let identity = (paste == Some(AppPaste::Paste) && lookup(&app).is_none())
                .then_some(identity)
                .flatten();
            let noted = previous.is_some_and(|entry| entry.noted);
            apps.retain(|entry| entry.app != app);
            apps.push(Remembered {
                app,
                output,
                paste,
                identity,
                noted,
            });
        })?;
        if chose_output {
            self.answer_earlier_formats();
        }
        Ok(())
    }

    fn unreadable(&self) -> bool {
        self.state.lock().map_or(true, |state| state.1)
    }

    /// Whether a paste into `app` should show the one-time plugin note. It
    /// depends only on the app ID. Saying yes records it, so it shows once; a
    /// failed save says no, and the next paste tries again.
    pub(crate) fn take_plugin_note(&self, app: &AppId) -> bool {
        if !has_citation_plugins(app) || self.unreadable() {
            return false;
        }
        let app = normalized(app);
        let Ok(state) = self.state.lock() else { return false };
        if state.0.iter().any(|entry| entry.app == app && entry.noted) {
            return false;
        }
        drop(state);
        let mut first = false;
        let saved = self.change(|apps| {
            if let Some(entry) = apps.iter_mut().find(|entry| entry.app == app) {
                first = !entry.noted;
                entry.noted = true;
            } else {
                first = true;
                apps.push(Remembered {
                    app,
                    output: None,
                    paste: None,
                    identity: None,
                    noted: true,
                });
            }
        });
        first && saved.is_ok()
    }

    fn change(&self, change: impl FnOnce(&mut Vec<Remembered>)) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("app choices are unavailable"))?;
        // Earlier choices that could not be kept as a copy are never overwritten.
        if self.earlier.lock().map_or(true, |earlier| earlier.1) {
            return Err(io::Error::other("earlier format choices could not be kept"));
        }
        let mut apps = state.0.clone();
        change(&mut apps);
        let excess = apps.len().saturating_sub(MAX_APPS);
        apps.drain(..excess);
        let recovery = state.1;
        write(&self.path, &apps, recovery)?;
        *state = (apps, recovery);
        Ok(())
    }
}

/// Whether the file holds an output this Ibis cannot read, as 0.2 wrote them.
fn earlier_formats(path: &Path) -> bool {
    let Ok(text) = fs::read(path) else { return false };
    let Ok(file) = serde_json::from_slice::<serde_json::Value>(&text) else {
        return false;
    };
    file["apps"].as_array().is_some_and(|apps| {
        apps.iter().any(|entry| {
            entry
                .get("output")
                .is_some_and(|output| !output.is_null() && output.as_str().and_then(crate::output::listed).is_none())
        })
    })
}

fn read(path: &Path) -> io::Result<MemoryFile> {
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
        if entry
            .identity
            .as_ref()
            .is_some_and(|identity| identity.len() != 64 || !identity.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
    }
    Ok(file)
}

/// Write the whole file beside the old one, then swap it in, so a crash
/// leaves either file whole.
fn write(path: &Path, apps: &[Remembered], recovery: bool) -> io::Result<()> {
    let json = serde_json::to_string_pretty(&MemoryFile {
        version: VERSION,
        apps: apps.to_vec(),
        recovery,
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
    editor: State<'_, crate::editor_endpoint::Pending>,
) -> Result<PickerDestination, String> {
    // The editor already said what it wants and takes the citation itself.
    let last = if editor.current().is_some() {
        None
    } else {
        crate::paste::last_app(&app)
    };
    let context = crate::writing_projects::destination_context(&app, last.as_ref().map(|(id, _, _)| id))?;
    let context_version = Some(context.version);
    let writing_project = context.active.map(|project| project.view());
    let writing_context = editor.current().is_none().then_some(context.edit);
    let Some((id, name, app_identity)) = last else {
        return Ok(PickerDestination {
            app: None,
            app_id: None,
            app_identity: None,
            automatic: None,
            remembered: None,
            unreadable: false,
            can_paste: false,
            output_question: None,
            paste: None,
            paste_question: false,
            can_choose_paste: false,
            writing_project,
            context_version,
            writing_context,
        });
    };
    let settings = settings::load(&settings_file.path).settings;
    let can_paste = crate::paste::intends_paste(&app, settings.paste.unwrap_or(true));
    let default = crate::output::stored(settings.output.as_deref());
    let (remembered, _) = memory.get(&id);
    let policy = memory.paste_for(&id, app_identity.as_deref());
    let can_choose_paste = crate::paste::may_choose_paste(&app, settings.paste.unwrap_or(true));
    // Only an app that can answer sees the warning; the others never paste anyway.
    let unreadable = policy.is_err() && can_choose_paste;
    let ordinary = lookup(&id).is_none_or(|entry| entry.safety == PasteSafety::Normal);
    let paste = ordinary.then(|| policy.ok().flatten()).flatten();
    let paste_question = lookup(&id).is_none() && paste.is_none() && can_choose_paste;
    Ok(PickerDestination {
        app: Some(name),
        app_id: Some(id.normalized()),
        app_identity,
        automatic: automatic(&id, default),
        remembered,
        unreadable,
        can_paste,
        output_question: lookup(&id).and_then(|app| app.output_question),
        paste,
        paste_question,
        can_choose_paste,
        writing_project,
        context_version,
        writing_context,
    })
}

/// Remember a format, a delivery choice or both for the captured app. Whichever
/// is absent keeps its saved value; only a delivery choice revokes outstanding proofs.
#[tauri::command]
#[specta::specta]
pub(crate) fn remember_app_output(
    app: AppHandle,
    memory: State<'_, AppMemory>,
    app_id: String,
    output: Option<String>,
    paste: Option<AppPaste>,
    identity: Option<String>,
) -> Result<(), String> {
    // Only an id the engine lists is remembered; anything else changes nothing.
    let output = output
        .map(|id| crate::output::listed(&id).ok_or("this format is not available; choose another"))
        .transpose()?;
    let said = memory.says_earlier_formats();
    crate::paste::change_destination(&app, paste.is_some(), |captured| {
        let captured = captured.ok_or("no app to remember a choice for")?;
        let id = captured.target.app.as_ref().ok_or("no app to remember a choice for")?;
        if id.normalized() != app_id || captured.app_identity != identity {
            return Err("the app changed; open the output menu again".into());
        }
        memory
            .set_bound_choice(id, output, paste, identity)
            .map_err(|_| "the choice for this app could not be saved".into())
    })?;
    // A chosen format answers the notice about earlier format choices.
    if said && !memory.says_earlier_formats() {
        crate::library::refresh_settings_notice(&app, &app.state::<SettingsFile>());
    }
    Ok(())
}

#[cfg(test)]
fn remember_for(memory: &AppMemory, id: &AppId, expected: &str, output: Option<OutputRef>) -> Result<(), String> {
    remember_choice_for(memory, id, expected, output, None)
}

#[cfg(test)]
fn remember_choice_for(
    memory: &AppMemory,
    id: &AppId,
    expected: &str,
    output: Option<OutputRef>,
    paste: Option<AppPaste>,
) -> Result<(), String> {
    if id.normalized() != expected {
        return Err("the app changed; open the output menu again".into());
    }
    memory
        .set_choice(id, output, paste)
        .map_err(|_| "the choice for this app could not be saved".into())
}

#[cfg(test)]
fn test_identity(app: &AppId) -> Option<String> {
    let path = if app.system == ibis_core::destination::System::Windows {
        app.id.as_str()
    } else {
        "/invented/Writer.app"
    };
    crate::paste::capture::app_identity(app.system, &app.id, path)
}

/// Read the per-app file once the settings location is known.
pub(crate) fn manage(app: &AppHandle, settings: &Path) {
    let memory = AppMemory::open(memory_path(settings));
    memory.note_earlier_settings(settings);
    app.manage(memory);
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

    fn word() -> AppId {
        mac("com.microsoft.word")
    }

    fn memory_in(directory: &tempfile::TempDir) -> AppMemory {
        AppMemory::open(directory.path().join("apps.json"))
    }

    #[test]
    fn format_choices_from_0_2_are_kept_as_a_copy_said_once_and_defaults_apply() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("apps.json");
        // As Ibis 0.2.7 wrote it: an output was an object, not an engine id.
        // Marked for delivery recovery too: the formats are kept all the same.
        let earlier = serde_json::to_vec(&serde_json::json!({ "version": 1, "recovery": true, "apps": [
            { "app": serde_json::to_value(word())?, "output": { "style": "ieee" }, "noted": true },
            { "app": serde_json::to_value(mac("org.example.writer"))?, "output": { "key": "pandoc" } },
        ] }))?;
        fs::write(&path, &earlier)?;
        // An interrupted copy from an earlier start is not a copy.
        fs::write(directory.path().join("apps.json.earlier.partial"), "inven")?;
        let memory = AppMemory::open(path.clone());
        assert_eq!(
            fs::read(directory.path().join("apps.json.earlier"))?,
            earlier,
            "kept whole"
        );
        assert_eq!(memory.get(&word()).0, None, "the defaults apply");
        assert!(!directory.path().join("apps.json.earlier.partial").exists());
        assert!(memory.says_earlier_formats());
        let notice = crate::preferences::settings_notice(
            &directory.path().join("settings.json"),
            &settings::load(&directory.path().join("settings.json")),
            memory.says_earlier_formats(),
        );
        assert_eq!(
            notice.as_deref(),
            Some(
                "Format choices from an earlier Ibis could not be read, so Ibis uses its defaults. Choose them again."
            )
        );
        // The plugin note stays shown, and saving keeps the copy as it was.
        assert!(!memory.take_plugin_note(&word()));
        memory.set(&mac("org.example.writer"), crate::output::listed("pandoc"))?;
        assert!(!memory.says_earlier_formats(), "a chosen format answers it");
        assert_eq!(fs::read(directory.path().join("apps.json.earlier"))?, earlier);
        // Said once: the next start keeps the first copy and says nothing.
        fs::write(&path, &earlier)?;
        assert!(!AppMemory::open(path).says_earlier_formats());
        assert_eq!(fs::read(directory.path().join("apps.json.earlier"))?, earlier);
        Ok(())
    }

    #[test]
    fn a_0_2_default_format_is_kept_and_said_once_until_any_format_is_chosen() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let settings = directory.path().join("settings.json");
        let earlier = br#"{"schema_version": 2, "citation_style": "ieee"}"#;
        fs::write(&settings, earlier)?;
        let memory = memory_in(&directory);
        memory.note_earlier_settings(&settings);
        assert!(memory.says_earlier_formats());
        assert_eq!(fs::read(directory.path().join("settings.json.earlier"))?, earlier);
        // Any chosen default answers it, Pandoc's included, which is stored as none.
        memory.answer_earlier_formats();
        assert!(!memory.says_earlier_formats());
        let next_start = memory_in(&directory);
        next_start.note_earlier_settings(&settings);
        assert!(!next_start.says_earlier_formats());
        Ok(())
    }

    #[test]
    fn earlier_choices_are_never_overwritten_when_their_copy_cannot_be_kept() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("apps.json");
        let earlier = serde_json::to_vec(&serde_json::json!({ "version": 1, "apps": [
            { "app": serde_json::to_value(word())?, "output": { "style": "ieee" } },
        ] }))?;
        fs::write(&path, &earlier)?;
        fs::create_dir(directory.path().join("apps.json.earlier"))?;
        fs::write(directory.path().join("apps.json.earlier").join("blocker"), "invented")?;
        let memory = AppMemory::open(path.clone());
        assert!(
            memory
                .set(&mac("org.example.other"), crate::output::listed("pandoc"))
                .is_err()
        );
        assert_eq!(fs::read(&path)?, earlier);
        Ok(())
    }

    #[test]
    fn the_plugin_note_shows_once_per_app_and_survives_a_restart() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = memory_in(&directory);
        assert!(memory.take_plugin_note(&word()));
        assert!(!memory.take_plugin_note(&word()));
        assert!(memory.take_plugin_note(&mac("com.apple.iwork.pages")), "per app");
        assert!(!memory_in(&directory).take_plugin_note(&word()));
        Ok(())
    }

    #[test]
    fn the_plugin_note_survives_later_choices_for_the_app() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = memory_in(&directory);
        assert!(memory.take_plugin_note(&word()));
        memory.set(
            &word(),
            Some(crate::output::listed("apa").unwrap_or_else(|| panic!("listed"))),
        )?;
        memory.set_choice(&word(), None, Some(AppPaste::CopyOnly))?;
        assert!(!memory.take_plugin_note(&word()), "a choice keeps the note shown");
        let reopened = memory_in(&directory);
        assert_eq!(
            reopened.get(&word()).0,
            Some(crate::output::listed("apa").unwrap_or_else(|| panic!("listed")))
        );
        assert!(!reopened.take_plugin_note(&word()));
        Ok(())
    }

    #[test]
    fn a_shown_note_leaves_the_file_alone_on_every_later_paste() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = memory_in(&directory);
        assert!(memory.take_plugin_note(&word()));
        let path = directory.path().join("apps.json");
        fs::remove_file(&path)?;
        assert!(!memory.take_plugin_note(&word()));
        assert!(!path.exists(), "an already noted app writes nothing");
        Ok(())
    }

    #[test]
    fn browsers_and_unknown_apps_never_get_the_note_or_a_file_entry() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = memory_in(&directory);
        assert!(!memory.take_plugin_note(&mac("com.google.chrome")));
        assert!(!memory.take_plugin_note(&mac("org.example.unknown")));
        assert!(!directory.path().join("apps.json").exists());
        Ok(())
    }

    #[test]
    fn a_failed_save_shows_no_note_so_it_cannot_repeat() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = memory_in(&directory);
        // A directory in the file's place: nothing can be saved over it.
        let blocked = directory.path().join("apps.json");
        fs::create_dir(&blocked)?;
        fs::write(blocked.join("kept"), "")?;
        assert!(!memory.take_plugin_note(&word()));
        Ok(())
    }

    #[test]
    fn the_note_keeps_only_a_flag_beside_the_app_id() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        memory_in(&directory).take_plugin_note(&word());
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(directory.path().join("apps.json"))?)?;
        assert_eq!(
            saved["apps"][0],
            serde_json::json!({ "app": { "system": "macos", "id": "com.microsoft.word" }, "output": null, "noted": true })
        );
        Ok(())
    }

    #[test]
    fn another_executable_with_the_same_basename_does_not_inherit_paste() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = AppMemory::open(directory.path().join("apps.json"));
        let first = AppId {
            system: System::Windows,
            id: r"C:\Invented\Writer\editor.exe".into(),
        };
        let other = AppId {
            system: System::Windows,
            id: r"C:\Invented\Viewer\editor.exe".into(),
        };
        memory.set_choice(&first, None, Some(AppPaste::Paste))?;
        assert_eq!(memory.paste(&first), Ok(Some(AppPaste::Paste)));
        assert_eq!(memory.paste(&other), Ok(None));
        Ok(())
    }

    #[test]
    fn a_reused_mac_bundle_id_or_missing_identity_requires_new_approval() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("apps.json");
        let app = mac("org.example.writer");
        let first = crate::paste::capture::app_identity(System::Macos, &app.id, "/invented/Writer.app");
        let other = crate::paste::capture::app_identity(System::Macos, &app.id, "/invented/Viewer.app");
        let memory = AppMemory::open(path.clone());
        assert!(
            memory
                .set_bound_choice(&app, None, Some(AppPaste::Paste), None)
                .is_err()
        );
        memory.set_bound_choice(&app, None, Some(AppPaste::Paste), first.clone())?;
        let reopened = AppMemory::open(path.clone());
        assert_eq!(reopened.paste_for(&app, first.as_deref()), Ok(Some(AppPaste::Paste)));
        assert_eq!(reopened.paste_for(&app, other.as_deref()), Ok(None));
        assert_eq!(reopened.paste_for(&app, None), Ok(None));
        // A format save from the replacement program cannot carry the old approval forward.
        reopened.set_bound_choice(
            &app,
            Some(crate::output::listed("pandoc").unwrap_or_else(|| panic!("listed"))),
            None,
            other.clone(),
        )?;
        assert_eq!(reopened.paste_for(&app, other.as_deref()), Ok(None));
        let json = fs::read_to_string(path)?;
        assert!(!json.contains("/invented/"), "only an identity digest is saved");
        Ok(())
    }

    #[test]
    fn unrelated_format_saves_cannot_recover_lost_copy_only_rules() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("apps.json");
        let word = mac("com.microsoft.word");
        let other = mac("org.example.other");
        AppMemory::open(path.clone()).set_choice(&word, None, Some(AppPaste::CopyOnly))?;
        fs::write(&path, "damaged invented preferences")?;
        let memory = AppMemory::open(path.clone());
        assert_eq!(memory.paste(&word), Err(CopyReason::PasteUnavailable));
        memory.set(
            &other,
            Some(crate::output::listed("pandoc").unwrap_or_else(|| panic!("listed"))),
        )?;
        assert_eq!(memory.paste(&word), Err(CopyReason::PasteUnavailable));
        let reopened = AppMemory::open(path.clone());
        assert_eq!(reopened.paste(&word), Err(CopyReason::PasteUnavailable));
        reopened.set_choice(&other, None, Some(AppPaste::CopyOnly))?;
        assert_eq!(reopened.paste(&other), Ok(Some(AppPaste::CopyOnly)));
        assert_eq!(reopened.paste(&word), Err(CopyReason::PasteUnavailable));
        reopened.set_choice(&word, None, Some(AppPaste::CopyOnly))?;
        assert_eq!(reopened.paste(&word), Ok(Some(AppPaste::CopyOnly)));
        assert_eq!(AppMemory::open(path).paste(&word), Ok(Some(AppPaste::CopyOnly)));
        Ok(())
    }

    #[test]
    fn paste_choices_and_formats_stay_separate_and_durable() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("apps.json");
        let app = mac("org.example.Writer");
        let format = Some(crate::output::listed("pandoc").unwrap_or_else(|| panic!("listed")));
        for paste in [AppPaste::Paste, AppPaste::CopyOnly] {
            let memory = AppMemory::open(path.clone());
            memory.set_choice(&app, format, Some(paste))?;
            let reopened = AppMemory::open(path.clone());
            assert_eq!(reopened.get(&mac("ORG.EXAMPLE.WRITER")), (format, false));
            assert_eq!(reopened.paste(&mac("ORG.EXAMPLE.WRITER")), Ok(Some(paste)));
            // A later format pick preserves delivery; a format alone never grants it.
            reopened.set(
                &app,
                Some(crate::output::listed("cite").unwrap_or_else(|| panic!("listed"))),
            )?;
            assert_eq!(reopened.paste(&app), Ok(Some(paste)));
            // A delivery answer keeps the format the person chose.
            reopened.set_choice(&app, None, Some(paste))?;
            assert_eq!(
                AppMemory::open(path.clone()).get(&app).0,
                Some(crate::output::listed("cite").unwrap_or_else(|| panic!("listed")))
            );
            reopened.set(&app, format)?;
        }
        let other = mac("org.example.other");
        let memory = AppMemory::open(path);
        memory.set(&other, format)?;
        assert_eq!(memory.paste(&other), Ok(None));
        Ok(())
    }

    #[test]
    fn a_choice_with_nothing_in_it_is_refused_and_keeps_what_was_saved() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("apps.json");
        let memory = AppMemory::open(path.clone());
        let app = mac("org.example.Writer");
        let format = Some(crate::output::listed("pandoc").unwrap_or_else(|| panic!("listed")));
        memory.set_choice(&app, format, Some(AppPaste::CopyOnly))?;
        assert!(memory.set(&app, None).is_err());
        let reopened = AppMemory::open(path);
        assert_eq!(reopened.get(&app), (format, false));
        assert_eq!(reopened.paste(&app), Ok(Some(AppPaste::CopyOnly)));
        Ok(())
    }

    #[test]
    fn failed_or_stale_paste_choices_never_grant_approval() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("apps.json");
        let memory = AppMemory::open(path.clone());
        let app = mac("org.example.Writer");
        assert!(remember_choice_for(&memory, &app, "org.example.other", None, Some(AppPaste::Paste)).is_err());
        assert_eq!(memory.paste(&app), Ok(None));
        memory.set_choice(&app, None, Some(AppPaste::CopyOnly))?;
        fs::remove_file(&path)?;
        fs::create_dir(&path)?;
        assert!(memory.set_choice(&app, None, Some(AppPaste::Paste)).is_err());
        assert_eq!(memory.paste(&app), Ok(Some(AppPaste::CopyOnly)));
        assert_eq!(AppMemory::open(path).paste(&app), Err(CopyReason::PasteUnavailable));
        Ok(())
    }

    #[test]
    fn no_refused_app_can_be_approved_by_a_destination_save() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let memory = AppMemory::open(directory.path().join("apps.json"));
        for entry in ibis_core::destination::APPS
            .iter()
            .filter(|entry| entry.safety != PasteSafety::Normal)
        {
            let app = AppId {
                system: entry.system,
                id: entry.id.into(),
            };
            assert!(
                memory.set_choice(&app, None, Some(AppPaste::Paste)).is_err(),
                "{}",
                entry.name
            );
            assert_eq!(memory.paste(&app), Ok(None));
        }
        Ok(())
    }

    #[test]
    fn each_kind_of_app_gets_its_output_and_unknown_apps_follow_the_default() {
        let id = |app: &AppId, default: &str| automatic(app, crate::output::stored(Some(default))).map(OutputRef::id);
        // A formatted app takes the default when it is a style, else the engine's style.
        assert_eq!(id(&mac("com.microsoft.Word"), "pandoc"), Some("apa"));
        assert_eq!(id(&mac("com.microsoft.Word"), "ieee"), Some("ieee"));
        assert_eq!(
            id(&mac("com.microsoft.Word"), "typst"),
            Some(ibis_engine::outputs::DEFAULT_STYLE)
        );
        assert_eq!(id(&mac("md.obsidian"), "ieee"), Some("pandoc"));
        assert_eq!(id(&mac("texshop"), "pandoc"), Some("autocite"));
        let windows = AppId {
            system: System::Windows,
            id: r"C:\Program Files\TeXstudio\texstudio.exe".into(),
        };
        assert_eq!(id(&windows, "pandoc"), Some("autocite"));
        assert_eq!(id(&mac("com.apple.Terminal"), "pandoc"), None);
        assert_eq!(id(&mac("org.example.unknown"), "pandoc"), None);
    }

    #[test]
    fn an_output_this_ibis_does_not_list_reads_as_no_choice_and_keeps_delivery() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = memory_path(&directory.path().join("settings.json"));
        let word = mac("com.microsoft.Word");
        for output in [
            serde_json::json!("a-later-output"),
            serde_json::json!({"key": "pandoc"}),
            serde_json::json!(7),
        ] {
            let file = serde_json::json!({"version": 1, "apps": [
                {"app": {"system": "macos", "id": "com.microsoft.word"}, "output": output, "paste": "copy-only"},
            ]});
            fs::write(&path, file.to_string()).unwrap_or_else(|e| panic!("{e}"));
            let memory = AppMemory::open(path.clone());
            // The file stays readable: the delivery choice is kept, the format fails closed.
            assert_eq!(memory.get(&word), (None, false), "{output}");
            assert_eq!(memory.paste(&word), Ok(Some(AppPaste::CopyOnly)), "{output}");
        }
    }

    #[test]
    fn a_remembered_choice_survives_a_restart_and_a_new_pick_replaces_it() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = memory_path(&directory.path().join("settings.json"));
        let memory = AppMemory::open(path.clone());
        assert_eq!(memory.get(&mac("com.microsoft.Word")), (None, false));
        let apa = crate::output::listed("apa").unwrap_or_else(|| panic!("listed"));
        memory
            .set(&mac("com.microsoft.Word"), Some(apa))
            .unwrap_or_else(|e| panic!("{e}"));
        let reopened = AppMemory::open(path.clone());
        // IDs compare as the platform does: bundle IDs without case.
        assert_eq!(reopened.get(&mac("com.microsoft.word")), (Some(apa), false));
        let typst = crate::output::listed("typst").unwrap_or_else(|| panic!("listed"));
        reopened
            .set(&mac("com.microsoft.word"), Some(typst))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            AppMemory::open(path).get(&mac("com.microsoft.Word")),
            (Some(typst), false)
        );
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
                    Some(crate::output::listed("pandoc").unwrap_or_else(|| panic!("listed"))),
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
            serde_json::json!({ "app": { "system": "macos", "id": "org.example.app1" }, "output": "pandoc" })
        );
    }

    #[test]
    fn an_unreadable_file_keeps_delivery_recovery_after_format_choices() {
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
                .set(&mac("com.microsoft.Word"), Some(crate::output::listed("cite").unwrap_or_else(|| panic!("listed"))))
                .unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(
                AppMemory::open(path.clone()).get(&mac("com.microsoft.Word")),
                (Some(crate::output::listed("cite").unwrap_or_else(|| panic!("listed"))), true)
            );
        }
    }
    #[test]
    fn ambiguous_browser_and_editor_ids_have_no_inferred_document_syntax() {
        for id in ["com.google.Chrome", "com.apple.Safari", "com.microsoft.VSCode"] {
            assert_eq!(automatic(&mac(id), crate::output::stored(None)), None);
        }
        for id in ["chrome.exe", "msedge.exe", "code.exe"] {
            assert_eq!(
                automatic(
                    &AppId {
                        system: System::Windows,
                        id: id.into()
                    },
                    crate::output::stored(None)
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
                Some(crate::output::listed("cite").unwrap_or_else(|| panic!("listed")))
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
        let cite = crate::output::listed("cite").unwrap_or_else(|| panic!("listed"));
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
        assert!(
            memory
                .set(
                    &mac("example.browser"),
                    Some(crate::output::listed("typst").unwrap_or_else(|| panic!("listed")))
                )
                .is_err()
        );
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
            .set(
                &mac("example.browser"),
                Some(crate::output::listed("cite").unwrap_or_else(|| panic!("listed"))),
            )
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
