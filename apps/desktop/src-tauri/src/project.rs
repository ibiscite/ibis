//! The project bibliography: one read-only `.bib` the person chose, which
//! results are checked against and which is never searched.
//!
//! One slot owns the choice. Each choice takes a new generation, and a read,
//! a status and an entry copy all carry the generation they began with, so a
//! slow read of an earlier file can never answer for a later one. The file is
//! read when it is chosen, at launch and when the picker opens: nothing polls.

use std::path::PathBuf;
use std::sync::Mutex;

use ibis_engine::{EntryRefusal, EntryResult, KeyCheck, ProjectRead, project_name};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::diagnostics::{NativeIssue, report};
use crate::library::commands::RecentIdentity;
use crate::library::{Library, ProjectMark};
use crate::picker::ChooserOpen;
use crate::preferences::SettingsFile;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProjectState {
    /// No answer yet for this choice.
    Reading,
    /// Every entry could be read.
    Ready,
    /// Part of the file could not be read, so nothing can be called missing.
    Damaged,
    /// The newest read failed.
    Failed,
}

/// What the picker may show. A file name, never a folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct ProjectStatus {
    pub(crate) generation: u32,
    pub(crate) name: String,
    pub(crate) state: ProjectState,
    /// Answers come from an earlier read, because the newest one failed.
    pub(crate) last_good: bool,
}

/// The newest read of the current choice, for one reply to use throughout.
#[derive(Clone)]
pub(crate) struct Current {
    pub(crate) generation: u32,
    pub(crate) name: String,
    pub(crate) read: ProjectRead,
}

struct Ticket {
    generation: u32,
    path: PathBuf,
    previous: Option<ProjectRead>,
}

#[derive(Default)]
struct Slot {
    generation: u32,
    path: Option<PathBuf>,
    read: Option<ProjectRead>,
    reading: bool,
}

#[derive(Default)]
pub(crate) struct ProjectBibliography(Mutex<Slot>);

impl ProjectBibliography {
    fn lock(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Replace the choice. Nothing from the old file carries over.
    fn choose(&self, path: Option<PathBuf>) -> Option<Ticket> {
        let mut slot = self.lock();
        slot.generation = slot.generation.wrapping_add(1);
        slot.path = path;
        slot.read = None;
        slot.reading = false;
        Self::begin(&mut slot)
    }

    /// Read the current choice again, unless a read of it is running.
    fn check_again(&self) -> Option<Ticket> {
        let mut slot = self.lock();
        if slot.reading { None } else { Self::begin(&mut slot) }
    }

    fn begin(slot: &mut Slot) -> Option<Ticket> {
        let path = slot.path.clone()?;
        slot.reading = true;
        Some(Ticket {
            generation: slot.generation,
            path,
            previous: slot.read.clone(),
        })
    }

    /// Install a finished read, if its choice is still current. True when that
    /// changed what the picker shows; an unchanged file wakes nothing.
    fn finish(&self, generation: u32, read: ProjectRead) -> bool {
        let mut slot = self.lock();
        if slot.generation != generation {
            return false;
        }
        slot.reading = false;
        let unchanged = slot.read.as_ref().is_some_and(|old| old.same_answers(&read));
        slot.read = Some(read);
        !unchanged
    }

    pub(crate) fn status(&self) -> Option<ProjectStatus> {
        let slot = self.lock();
        let name = project_name(slot.path.as_deref()?);
        let (state, last_good) = match &slot.read {
            None => (ProjectState::Reading, false),
            Some(read) if read.problem().is_some() => (ProjectState::Failed, read.last_good()),
            Some(read) if read.bibliography().is_some_and(|good| !good.complete()) => (ProjectState::Damaged, false),
            Some(_) => (ProjectState::Ready, false),
        };
        Some(ProjectStatus {
            generation: slot.generation,
            name,
            state,
            last_good,
        })
    }

    /// The read a reply should use; `None` until the choice has one.
    pub(crate) fn current(&self) -> Option<Current> {
        let slot = self.lock();
        Some(Current {
            generation: slot.generation,
            name: project_name(slot.path.as_deref()?),
            read: slot.read.clone()?,
        })
    }

    fn is_current(&self, generation: u32) -> bool {
        self.lock().generation == generation
    }
}

impl Current {
    /// What a row shows. `None` for a reference with no key to compare.
    pub(crate) fn mark(&self, check: Option<KeyCheck>) -> Option<ProjectMark> {
        Some(ProjectMark::new(self.generation, &check?))
    }

    /// A sentence for a key that is not plainly in the file. These join the
    /// citation's own warnings, so the picker copies and stays open.
    pub(crate) fn warning(&self, key: &str, check: &KeyCheck) -> Option<String> {
        let name = &self.name;
        match check {
            KeyCheck::In { .. } => None,
            KeyCheck::Missing => Some(format!("{key} is missing from {name}.")),
            KeyCheck::CouldNotCheck => Some(format!("Couldn't check {key} against {name}.")),
            KeyCheck::Repeated { .. } => Some(format!("{key} appears more than once in {name}.")),
            KeyCheck::MayNameAnother { .. } => Some(format!("{key} may name another reference in {name}.")),
        }
    }
}

fn start_read(app: &AppHandle, ticket: Option<Ticket>) {
    let Some(ticket) = ticket else { return };
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let read = ProjectRead::read(&ticket.path, ticket.previous.as_ref());
        if handle.state::<ProjectBibliography>().finish(ticket.generation, read) {
            changed(&handle);
        }
    });
}

fn changed(app: &AppHandle) {
    report(app.emit("project:changed", ()), NativeIssue::LibraryEvent);
}

/// Read the saved choice at launch.
pub(crate) fn start(app: &AppHandle, saved: Option<PathBuf>) {
    let ticket = app.state::<ProjectBibliography>().choose(saved);
    start_read(app, ticket);
}

/// One read each time the picker opens, as for a library that has no watch.
pub(crate) fn watch_openings(app: &AppHandle) {
    use tauri::Listener;
    let handle = app.clone();
    app.listen("picker:shown", move |_| {
        let ticket = handle.state::<ProjectBibliography>().check_again();
        start_read(&handle, ticket);
    });
}

fn apply(app: &AppHandle, path: Option<PathBuf>) -> Result<(), String> {
    let saved = path.clone();
    app.state::<SettingsFile>()
        .update(move |settings| settings.project_bibliography = saved)
        .map_err(|_| "The project bibliography could not be saved. Try again.".to_owned())?;
    let ticket = app.state::<ProjectBibliography>().choose(path);
    changed(app);
    start_read(app, ticket);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn project_status(slot: State<'_, ProjectBibliography>) -> Option<ProjectStatus> {
    slot.status()
}

/// Choose the file with the system dialog; the interface never handles a path.
#[tauri::command]
#[specta::specta]
pub(crate) fn choose_project(app: AppHandle) {
    let Some(owner) = app.state::<ChooserOpen>().begin() else {
        return;
    };
    #[cfg(target_os = "macos")]
    crate::panel::activate_for_dialog(&app);
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Choose the project's bibliography")
        .add_filter("BibTeX and BibLaTeX", &["bib", "bibtex", "biblatex"]);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let handle = app.clone();
    dialog.pick_file(move |picked| {
        owner.during(|| {
            if let Some(Ok(path)) = picked.map(|path| path.into_path())
                && apply(&handle, Some(path)).is_err()
            {
                report(handle.emit("project:not-saved", ()), NativeIssue::LibraryEvent);
            }
        });
        let owner = handle.clone();
        report(
            handle.run_on_main_thread(move || {
                if let Some(window) = owner.get_webview_window("main") {
                    crate::picker::reconcile_suppressed_blur(&window, generation);
                }
            }),
            NativeIssue::Window,
        );
    });
}

/// Stop checking. The file itself is untouched.
#[tauri::command]
#[specta::specta]
pub(crate) fn stop_project(app: AppHandle) -> Result<(), String> {
    apply(&app, None)
}

/// How an entry copy ended; a refusal says how to get the entry another way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EntryCopy {
    Copied,
    NoOriginal,
    /// The file or the choice changed since this was offered.
    Changed,
    Repeated,
    Dependency,
}

impl From<EntryRefusal> for EntryCopy {
    fn from(refusal: EntryRefusal) -> Self {
        match refusal {
            EntryRefusal::NoOriginal => Self::NoOriginal,
            EntryRefusal::Changed => Self::Changed,
            EntryRefusal::Repeated => Self::Repeated,
            EntryRefusal::Dependency => Self::Dependency,
        }
    }
}

/// Copy a reference's original entry with its key, for adding to the project's
/// file. The text goes to the clipboard here and never through the web view.
#[tauri::command]
#[specta::specta]
pub(crate) async fn copy_project_entry(
    id: RecentIdentity,
    generation: u32,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
) -> Result<EntryCopy, String> {
    const FAILED: &str = "The entry could not be copied.";
    let file = library
        .lock()
        .map_err(|_| FAILED)?
        .file
        .clone()
        .ok_or("No library is open.")?;
    let result = tauri::async_runtime::spawn_blocking(move || file.original_entry(&id.0))
        .await
        .map_err(|_| FAILED)?
        .map_err(|_| FAILED)?;
    let text = match result {
        EntryResult::Entry { text, .. } => text,
        EntryResult::Refused { reason } => return Ok(reason.into()),
    };
    // Checked last: the choice may have changed during that read.
    if !app.state::<ProjectBibliography>().is_current(generation) {
        return Ok(EntryCopy::Changed);
    }
    crate::copy::write_plain(&app, &text)?;
    Ok(EntryCopy::Copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::contract::ItemRef;

    fn written(name: &str, text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap_or_else(|error| panic!("{error}"));
        (dir, path)
    }

    fn read(ticket: &Ticket) -> ProjectRead {
        ProjectRead::read(&ticket.path, ticket.previous.as_ref())
    }

    const A: &str = "@book{a2026,title={A}}\n";
    const B: &str = "@book{b2026,title={B}}\n";

    #[test]
    fn rows_and_warnings_use_the_key_a_citation_would_emit() {
        use crate::library::engine_file::FileLibrary;
        use ibis_engine::Cancellation;
        let (_library_dir, library) = written(
            "refs.bib",
            "@book{a2026,title={A}}\n@book{gone2026,title={Gone}}\n@book{clash2026,title={Library title}}\n",
        );
        let (_project_dir, project) = written("thesis.bib", "@book{a2026,title={A}}\n@book{clash2026,title={Other title}}\n");
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(project)).unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.finish(ticket.generation, read(&ticket)));
        let current = slot.current().unwrap_or_else(|| panic!("a read"));

        let file = FileLibrary::open(&library).unwrap_or_else(|_| panic!("a library"));
        let query = serde_json::from_value(serde_json::json!({"text":"2026","interpret_locators":false}))
            .unwrap_or_else(|error| panic!("{error}"));
        let reply = file
            .search(&query, 10, None, &Cancellation::default())
            .unwrap_or_else(|error| panic!("{error}"));
        let reply = file.mark_project(reply, Some(&current));
        let rows = serde_json::to_value(&reply).unwrap_or_else(|error| panic!("{error}"));
        let state = |key: &str| {
            rows["results"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["key"] == key))
                .map(|row| (row["project"]["state"].clone(), row["project"]["generation"].clone()))
        };
        let generation = serde_json::json!(ticket.generation);
        assert_eq!(state("a2026"), Some((serde_json::json!("in"), generation.clone())));
        assert_eq!(state("gone2026"), Some((serde_json::json!("missing"), generation.clone())));
        assert_eq!(state("clash2026"), Some((serde_json::json!("may-name-another"), generation)));

        let ids: Vec<ItemRef> = rows["results"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| serde_json::from_value(row["id"].clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        let mut warnings = file.project_warnings(&ids, &current);
        warnings.sort();
        assert_eq!(
            warnings,
            [
                "clash2026 may name another reference in thesis.bib.",
                "gone2026 is missing from thesis.bib."
            ],
            "the key that is in the file adds nothing"
        );
    }

    #[test]
    fn a_delayed_read_of_a_never_replaces_b() {
        let (_a_dir, a) = written("a.bib", A);
        let (_b_dir, b) = written("b.bib", B);
        let slot = ProjectBibliography::default();
        let first = slot.choose(Some(a)).unwrap_or_else(|| panic!("a ticket"));
        let second = slot.choose(Some(b)).unwrap_or_else(|| panic!("a ticket"));

        assert!(slot.finish(second.generation, read(&second)));
        assert!(
            !slot.finish(first.generation, read(&first)),
            "A finished last and was refused"
        );

        let current = slot.current().unwrap_or_else(|| panic!("a read"));
        assert_eq!(current.name, "b.bib");
        assert!(matches!(current.read.check("b2026", None), KeyCheck::In { .. }));
        assert_eq!(current.read.check("a2026", None), KeyCheck::Missing);
    }

    #[test]
    fn stopping_drops_a_read_still_running() {
        let (_dir, a) = written("a.bib", A);
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(a)).unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.choose(None).is_none());
        assert!(!slot.finish(ticket.generation, read(&ticket)));
        assert_eq!(slot.status(), None);
        assert!(slot.current().is_none());
    }

    #[test]
    fn a_new_choice_answers_nothing_until_its_own_read_finishes() {
        let (_a_dir, a) = written("a.bib", A);
        let (_b_dir, b) = written("b.bib", B);
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(a)).unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.finish(ticket.generation, read(&ticket)));
        slot.choose(Some(b));
        assert!(slot.current().is_none(), "A's answers must not stand in for B");
        assert_eq!(slot.status().map(|status| status.state), Some(ProjectState::Reading));
    }

    #[test]
    fn opening_again_reads_once_and_keeps_the_generation() {
        let (_dir, a) = written("a.bib", A);
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(a)).unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.check_again().is_none(), "one read at a time");
        assert!(slot.finish(ticket.generation, read(&ticket)));
        let again = slot.check_again().unwrap_or_else(|| panic!("a ticket"));
        assert_eq!(again.generation, ticket.generation);
        assert!(!slot.finish(again.generation, read(&again)), "an unchanged file wakes nothing");
        assert!(slot.check_again().is_some(), "and leaves no read running");
    }

    #[test]
    fn a_file_that_goes_away_keeps_its_last_good_copy_labelled() {
        let (_dir, a) = written("a.bib", A);
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(a.clone())).unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.finish(ticket.generation, read(&ticket)));
        assert_eq!(slot.status().map(|status| status.state), Some(ProjectState::Ready));

        std::fs::remove_file(&a).unwrap_or_else(|error| panic!("{error}"));
        let again = slot.check_again().unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.finish(again.generation, read(&again)));
        let status = slot.status().unwrap_or_else(|| panic!("a status"));
        assert_eq!((status.state, status.last_good), (ProjectState::Failed, true));
        let current = slot.current().unwrap_or_else(|| panic!("a read"));
        assert!(matches!(current.read.check("a2026", None), KeyCheck::In { .. }));
        assert_eq!(current.read.check("z2026", None), KeyCheck::CouldNotCheck);

        // After a restart there is no copy to keep, and still nothing is missing.
        let reopened = ProjectBibliography::default();
        let ticket = reopened.choose(Some(a)).unwrap_or_else(|| panic!("a ticket"));
        assert!(reopened.finish(ticket.generation, read(&ticket)));
        let status = reopened.status().unwrap_or_else(|| panic!("a status"));
        assert_eq!((status.state, status.last_good), (ProjectState::Failed, false));
        let current = reopened.current().unwrap_or_else(|| panic!("a read"));
        assert_eq!(current.read.check("a2026", None), KeyCheck::CouldNotCheck);
    }
}
