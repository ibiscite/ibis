//! The project bibliography: one read-only `.bib` the person chose, which
//! results are checked against and which is never searched.
//!
//! One slot owns the choice. Each choice takes a new generation, and a read,
//! a status and an entry copy all carry the generation they began with, so a
//! slow read of an earlier file can never answer for a later one. The file is
//! read when it is chosen, at launch and when the picker opens: nothing polls.

use std::path::PathBuf;
use std::sync::Mutex;

use ibis_core::contract::{CitedWarning, Message};
use ibis_engine::{EntryRefusal, EntryResult, KeyCheck, ProjectProblem, ProjectRead, project_name};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::diagnostics::{NativeIssue, report};
use crate::library::commands::RecentIdentity;
use crate::library::{Library, ProjectMark};
use crate::picker::ChooserOpen;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub(crate) struct ProjectStatus {
    pub(crate) generation: u32,
    /// Counts reads that changed an answer, which can happen with the status
    /// otherwise the same, as when an entry is added to a healthy file.
    pub(crate) revision: u32,
    pub(crate) name: String,
    pub(crate) state: ProjectState,
    /// Why the newest read failed, as a sentence ending in a full stop.
    pub(crate) reason: Option<String>,
    /// The first lines that could not be read.
    pub(crate) damaged_lines: Vec<u32>,
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
    revision: u32,
    path: Option<PathBuf>,
    read: Option<ProjectRead>,
    reading: bool,
    context: Option<String>,
    /// The bound receipt, its project observation and whether its keys are checked.
    prepared: Option<(String, (u32, u32), bool)>,
    output: Option<crate::output::OutputRef>,
    /// The choices being read now: the current one, and at most one replaced
    /// choice whose read has not finished yet.
    readers: Vec<u32>,
    pending_read: Option<Ticket>,
}

#[derive(Default)]
pub(crate) struct ProjectBibliography(Mutex<Slot>);

impl ProjectBibliography {
    fn lock(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Replace the choice. Nothing from the old file carries over.
    #[cfg(test)]
    fn choose(&self, path: Option<PathBuf>) -> Option<Ticket> {
        let mut slot = self.lock();
        slot.generation = slot.generation.wrapping_add(1);
        slot.path = path;
        slot.read = None;
        slot.reading = false;
        slot.prepared = None;
        slot.pending_read = None;
        Self::begin(&mut slot)
    }

    /// A replaced choice's read never holds up the current one, which starts
    /// beside it. At most two reads run and one latest choice waits, so quick
    /// switching cannot keep more than one discarded bibliography parse alive.
    fn enqueue_read(&self, ticket: Ticket) -> Option<Ticket> {
        let mut slot = self.lock();
        if ticket.generation != slot.generation {
            return None;
        }
        if slot.readers.len() < 2 && !slot.readers.contains(&ticket.generation) {
            slot.readers.push(ticket.generation);
            Some(ticket)
        } else {
            slot.pending_read = Some(ticket);
            None
        }
    }

    /// The reader of `finished` takes the waiting choice, if it may start now.
    fn next_read(&self, finished: u32) -> Option<Ticket> {
        let mut slot = self.lock();
        if let Some(index) = slot.readers.iter().position(|reading| *reading == finished) {
            slot.readers.swap_remove(index);
        }
        let current = slot.generation;
        let next = slot
            .pending_read
            .take()
            .filter(|ticket| ticket.generation == current)?;
        if slot.readers.contains(&current) {
            slot.pending_read = Some(next);
            return None;
        }
        slot.readers.push(current);
        Some(next)
    }

    pub(crate) fn version(&self) -> (u32, u32) {
        let slot = self.lock();
        (slot.generation, slot.revision)
    }

    pub(crate) fn capture(
        &self,
        expected: Option<(u32, u32)>,
        output: Option<crate::output::OutputRef>,
    ) -> Result<((u32, u32), Option<Current>), crate::library::CommandError> {
        let slot = self.lock();
        let version = (slot.generation, slot.revision);
        // An explicit editor owns its output and never borrows this bibliography.
        let Some(output) = output else {
            return Ok((version, None));
        };
        let valid = !slot
            .context
            .as_ref()
            .is_some_and(|context| context.starts_with("unavailable:"))
            && !(matches!(output.spec(), ibis_core::contract::OutputSpec::Key { .. })
                && slot.path.is_some()
                && (slot.read.is_none() || slot.reading))
            && expected == Some(version)
            && slot.output.is_none_or(|current| current == output);
        if !valid {
            return Err(crate::library::CommandError::Cancelled);
        }
        Ok((version, Self::current_in(&slot)))
    }

    /// A native receipt belongs to one immutable project observation, including
    /// the state with no bibliography. Equal output after A → B → A is new work.
    pub(crate) fn bind_preview(
        &self,
        engine: &ibis_engine::Engine,
        preview: &ibis_core::contract::Preview,
        version: (u32, u32),
    ) -> Result<(), crate::library::CommandError> {
        let mut slot = self.lock();
        if (slot.generation, slot.revision) != version {
            return Err(crate::library::CommandError::SnapshotChanged);
        }
        // The engine validates its own identity while the project binding is locked.
        // A late native completion cannot replace a newer receipt's context.
        let citation = engine.citations().prepared(
            ibis_engine::citations::SessionId::PICKER,
            &preview.ticket,
            &preview.content_hash,
        )?;
        slot.prepared = Some((preview.ticket.clone(), version, citation.uses_citation_keys()));
        Ok(())
    }

    /// Project changes and clipboard commits share this lock. Warnings and
    /// content are committed under the same context, before any clipboard I/O.
    /// A key citation waits for a read in progress: the answer it holds may be
    /// the one that read replaces.
    pub(crate) fn commit<T>(
        &self,
        ticket: &str,
        write: impl FnOnce(Option<Current>) -> Result<T, crate::library::CommandError>,
    ) -> Result<T, crate::library::CommandError> {
        let slot = self.lock();
        let valid = slot.prepared.as_ref().is_some_and(|(prepared, version, keys)| {
            prepared == ticket && *version == (slot.generation, slot.revision) && !(*keys && slot.reading)
        });
        if !valid {
            return Err(crate::library::CommandError::SnapshotChanged);
        }
        write(Self::current_in(&slot))
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
    /// changed an answer; an unchanged file asks nothing again.
    fn finish_with(&self, generation: u32, read: ProjectRead, retire_delivery: impl FnOnce()) -> Option<bool> {
        let mut slot = self.lock();
        if slot.generation != generation {
            return None;
        }
        slot.reading = false;
        let unchanged = slot.read.as_ref().is_some_and(|old| old.same_answers(&read));
        if !unchanged {
            // The same project→delivery boundary guards clipboard commits and
            // final native posting, including workers that already took a proof.
            retire_delivery();
        }
        slot.read = Some(read);
        if !unchanged {
            slot.revision = slot.revision.wrapping_add(1);
        }
        Some(!unchanged)
    }

    /// `None` when the read's choice was replaced; otherwise whether it changed
    /// an answer. Either way a current read has ended, which the status shows.
    pub(crate) fn finish_read<R: tauri::Runtime>(
        &self,
        app: &AppHandle<R>,
        generation: u32,
        read: ProjectRead,
    ) -> Option<bool> {
        self.finish_with(generation, read, || {
            let _ = crate::paste::change_destination(app, true, |_| Ok(()));
        })
    }

    #[cfg(test)]
    fn finish(&self, generation: u32, read: ProjectRead) -> bool {
        self.finish_with(generation, read, || {}) == Some(true)
    }

    pub(crate) fn status(&self) -> Option<ProjectStatus> {
        let slot = self.lock();
        let name = project_name(slot.path.as_deref()?);
        // A check of a file already read also reads: its old answer may not stand.
        let state = match &slot.read {
            _ if slot.reading => ProjectState::Reading,
            None => ProjectState::Reading,
            Some(read) if read.problem().is_some() => ProjectState::Failed,
            Some(read) if read.bibliography().is_some_and(|read| !read.complete()) => ProjectState::Damaged,
            Some(_) => ProjectState::Ready,
        };
        Some(ProjectStatus {
            generation: slot.generation,
            revision: slot.revision,
            name,
            state,
            reason: slot
                .read
                .as_ref()
                .and_then(ProjectRead::problem)
                .map(|problem| reason(problem).to_owned()),
            damaged_lines: slot.read.as_ref().map_or_else(Vec::new, |read| {
                read.damaged_lines().iter().take(5).map(|line| *line as u32).collect()
            }),
        })
    }

    /// The read a reply should use; `None` until the choice has one.
    pub(crate) fn current(&self) -> Option<Current> {
        let slot = self.lock();
        Self::current_in(&slot)
    }

    fn current_in(slot: &Slot) -> Option<Current> {
        Some(Current {
            generation: slot.generation,
            name: project_name(slot.path.as_deref()?),
            read: slot.read.clone()?,
        })
    }

    fn commit_generation<T>(&self, generation: u32, write: impl FnOnce() -> T) -> Option<T> {
        let slot = self.lock();
        (slot.generation == generation).then(write)
    }
}

fn reason(problem: ProjectProblem) -> &'static str {
    match problem {
        ProjectProblem::Unavailable => "It is not there, or not on this computer yet.",
        ProjectProblem::NotAFile => "It is not a file.",
        ProjectProblem::TooLarge => "It is larger than 64 MiB.",
        ProjectProblem::NotUtf8 => "It is not UTF-8 text.",
        ProjectProblem::Changing => "It is still changing.",
        ProjectProblem::NotBibtex => "It must be a BibTeX or BibLaTeX file.",
    }
}

impl Current {
    /// What a row shows. `None` for a reference with no key to compare.
    pub(crate) fn mark(&self, check: Option<KeyCheck>) -> Option<ProjectMark> {
        Some(ProjectMark::new(self.generation, &check?))
    }

    /// The warning for cited item `item`, when its key is not plainly in the
    /// file. These join the citation's own warnings, so the picker copies and
    /// stays open, and each shows with its reference.
    pub(crate) fn warning(&self, item: usize, key: &str, check: &KeyCheck) -> Option<CitedWarning> {
        let state = match check {
            KeyCheck::In { .. } => return None,
            KeyCheck::Missing => "missing",
            KeyCheck::CouldNotCheck => "could-not-check",
            KeyCheck::Repeated { .. } => "repeated",
            KeyCheck::MayNameAnother { .. } => "may-name-another",
        };
        let args = [("key", key), ("name", &self.name), ("state", state)];
        Some(CitedWarning {
            item: Some(item),
            message: Message {
                id: "app-project-warning".into(),
                args: args.map(|(name, value)| (name.to_owned(), value.to_owned())).into(),
                context: None,
            },
        })
    }
}

fn start_read(app: &AppHandle, ticket: Option<Ticket>) {
    let Some(ticket) = ticket else { return };
    let Some(mut ticket) = app.state::<ProjectBibliography>().enqueue_read(ticket) else {
        return;
    };
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        loop {
            let read = ProjectRead::read(&ticket.path, ticket.previous.as_ref());
            let project = handle.state::<ProjectBibliography>();
            if project.finish_read(&handle, ticket.generation, read).is_some() {
                changed(&handle);
            }
            match project.next_read(ticket.generation) {
                Some(next) => ticket = next,
                None => break,
            }
        }
    });
}

fn changed(app: &AppHandle) {
    report(
        app.emit("project:changed", app.state::<ProjectBibliography>().version()),
        NativeIssue::LibraryEvent,
    );
}

/// Install output ownership and its bibliography together. The same lock order
/// as copy (project, then delivery) makes switching revoke an old paste proof.
pub(crate) fn activate_context(
    app: &AppHandle,
    context: String,
    path: Option<PathBuf>,
    output: Option<crate::output::OutputRef>,
) {
    let project = app.state::<ProjectBibliography>();
    let ticket = {
        let mut slot = project.lock();
        if slot.context.as_ref() == Some(&context) && slot.path == path && slot.output == output {
            return;
        }
        slot.context = Some(context);
        slot.generation = slot.generation.wrapping_add(1);
        slot.path = path;
        slot.output = output;
        slot.read = None;
        slot.reading = false;
        slot.prepared = None;
        slot.pending_read = None;
        // A poisoned delivery owner cannot post keys; still retire our receipt.
        let _ = crate::paste::change_destination(app, true, |_| Ok(()));
        ProjectBibliography::begin(&mut slot)
    };
    changed(app);
    start_read(app, ticket);
}

/// One read each time the picker opens, as for a library that has no watch.
pub(crate) fn watch_openings(app: &AppHandle) {
    use tauri::Listener;
    let handle = app.clone();
    app.listen("picker:shown", move |_| read_again(&handle));
}

/// Check the current choice again. The picker hears that it is reading, so a
/// key citation waits for the new answer instead of using the old one.
fn read_again(app: &AppHandle) {
    let ticket = app.state::<ProjectBibliography>().check_again();
    if ticket.is_some() {
        changed(app);
    }
    start_read(app, ticket);
}

fn apply(app: &AppHandle, path: Option<PathBuf>) -> Result<(), crate::library::CommandError> {
    let owner = crate::writings::bibliography_owner(app)?;
    crate::writings::set_bibliography(app, &owner, path)
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn project_status(slot: State<'_, ProjectBibliography>) -> Option<ProjectStatus> {
    slot.status()
}

/// Recheck only after a person's explicit action; never a background timer.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn retry_project(app: AppHandle) {
    // A project changed elsewhere, by the command line for one, is used as
    // saved; a changed bibliography starts its own read here.
    let captured = crate::paste::last_app(&app);
    let _ = crate::writings::refresh(&app, captured.as_ref().map(|(id, _, _)| id));
    read_again(&app);
}

/// Choose the file with the system dialog; the interface never handles a path.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn choose_project(app: AppHandle) {
    let Some(owner) = app.state::<ChooserOpen>().begin() else {
        return;
    };
    #[cfg(target_os = "macos")]
    crate::panel::activate_for_dialog(&app);
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let Ok(bibliography_owner) = crate::writings::bibliography_owner(&app) else {
        let _ = app.emit("project:not-saved", ());
        return;
    };
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Choose the writing's bibliography")
        .add_filter("BibTeX and BibLaTeX", &["bib", "bibtex", "biblatex"]);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let handle = app.clone();
    dialog.pick_file(move |picked| {
        owner.during(|| {
            if let Some(Ok(path)) = picked.map(|path| path.into_path())
                && crate::writings::set_bibliography(&handle, &bibliography_owner, Some(path)).is_err()
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
#[ibis_desktop_macros::command]
pub(crate) fn stop_project(app: AppHandle) -> Result<(), crate::library::CommandError> {
    apply(&app, None)
}

/// How an entry copy ended; a refusal says how to get the entry another way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
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
#[ibis_desktop_macros::command]
pub(crate) async fn copy_project_entry(
    id: RecentIdentity,
    generation: u32,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
) -> Result<EntryCopy, crate::library::CommandError> {
    let file = library
        .lock()
        .map_err(|_| crate::errors::NativeOperation::CopyProjectEntry.failure())?
        .file
        .clone()
        .ok_or(crate::library::CommandError::SnapshotChanged)?;
    // An identity from before the library changed names nothing now.
    let Some(id) = id.0 else {
        return Ok(EntryCopy::Changed);
    };
    let result = tauri::async_runtime::spawn_blocking(move || file.original_entry(&id))
        .await
        .map_err(|_| crate::errors::NativeOperation::CopyProjectEntry.failure())??;
    let text = match result {
        EntryResult::Entry { text, .. } => text,
        EntryResult::Refused { reason } => return Ok(reason.into()),
    };
    // Checked last: the choice may have changed during that read.
    let captured = crate::paste::last_app(&app);
    crate::writings::refresh(&app, captured.as_ref().map(|(id, _, _)| id))?;
    app.state::<ProjectBibliography>()
        .commit_generation(generation, || {
            crate::copy::write_plain(&app, &text).map_err(|_| crate::library::CommandError::ClipboardUnavailable)?;
            Ok(EntryCopy::Copied)
        })
        .unwrap_or(Ok(EntryCopy::Changed))
}

pub(crate) fn preview_warnings(
    engine: &ibis_engine::Engine,
    id: &str,
    hash: &str,
    project: Option<&Current>,
) -> Result<Vec<CitedWarning>, crate::library::CommandError> {
    let citation = engine
        .citations()
        .prepared(ibis_engine::citations::SessionId::PICKER, id, hash)?;
    Ok(project
        .filter(|_| citation.uses_citation_keys())
        .map(|project| {
            citation
                .project_checks(&project.read)
                .into_iter()
                .filter_map(|(item, key)| project.warning(item, &key.key, &key.check))
                .collect()
        })
        .unwrap_or_default())
}
pub(crate) fn preview_evidence(
    engine: &ibis_engine::Engine,
    id: &str,
    hash: &str,
    project: Option<&Current>,
) -> Result<Option<ibis_engine::ProjectAssessment>, crate::library::CommandError> {
    let citation = engine
        .citations()
        .prepared(ibis_engine::citations::SessionId::PICKER, id, hash)?;
    Ok(project
        .filter(|_| citation.uses_citation_keys())
        .map(|project| citation.project_evidence(&project.read)))
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

    fn binding(engine: Option<ibis_engine::Engine>) -> (ibis_engine::Engine, ibis_core::contract::Preview) {
        let engine = engine.unwrap_or_else(|| {
            let case: ibis_core::contract::GoldenCase =
                serde_json::from_str(include_str!("../../../../fixtures/cases/02-pandoc.json")).unwrap();
            ibis_engine::Engine::from_snapshot(std::sync::Arc::new(case.library))
        });
        let snapshot = engine.snapshot().unwrap();
        let row = &snapshot.objects()[0];
        let draft=serde_json::from_value(serde_json::json!({"items":[{"id":row.object.id(),"cite":row.fingerprints.cite}],"output":{"kind":"preset","name":"pandoc"}})).unwrap();
        let preview = engine
            .citations()
            .preview(
                ibis_engine::citations::SessionId::PICKER,
                draft,
                ibis_engine::Cancellation::default(),
            )
            .unwrap()
            .run()
            .unwrap();
        (engine, preview)
    }

    const A: &str = "@book{a2026,title={A}}\n";
    const B: &str = "@book{b2026,title={B}}\n";

    #[test]
    fn a_preview_cannot_write_after_a_project_switch_even_when_switching_back() {
        let slot = ProjectBibliography::default();
        slot.choose(None);
        let a = slot.version();
        let (engine, preview) = binding(None);
        slot.bind_preview(&engine, &preview, a).unwrap();
        let mut written = String::new();
        slot.commit(&preview.ticket, |_| {
            written.push_str("citation A");
            Ok(())
        })
        .unwrap();
        assert_eq!(written, "citation A");
        slot.choose(None);
        assert!(
            slot.commit(&preview.ticket, |_| {
                written.push_str("wrong");
                Ok(())
            })
            .is_err()
        );
        slot.choose(None);
        assert!(slot.bind_preview(&engine, &preview, a).is_err());
        assert!(
            slot.commit(&preview.ticket, |_| {
                written.push_str("wrong");
                Ok(())
            })
            .is_err()
        );
        assert_eq!(written, "citation A");
        let (engine, fresh) = binding(Some(engine));
        slot.bind_preview(&engine, &fresh, slot.version()).unwrap();
        slot.commit(&fresh.ticket, |_| {
            written.push_str(" fresh A");
            Ok(())
        })
        .unwrap();
        assert_eq!(written, "citation A fresh A");
    }

    #[test]
    fn a_delayed_preview_cannot_replace_a_newer_receipt() {
        let slot = ProjectBibliography::default();
        let version = slot.version();
        let (engine, older) = binding(None);
        let (engine, newer) = binding(Some(engine));
        slot.bind_preview(&engine, &newer, version).unwrap();
        assert!(slot.bind_preview(&engine, &older, version).is_err());
        slot.commit(&newer.ticket, |_| Ok(())).unwrap();
    }

    #[test]
    fn an_obsolete_read_never_holds_up_the_newest_choice() {
        let slot = ProjectBibliography::default();
        let a = slot.choose(Some("/invented/a.bib".into())).unwrap();
        let a = slot.enqueue_read(a).unwrap();
        // A is still being read when the person chooses B: B starts at once.
        let b = slot.choose(Some("/invented/b.bib".into())).unwrap();
        let b = slot.enqueue_read(b).unwrap_or_else(|| panic!("B waited for A"));
        assert!(slot.finish(b.generation, read(&b)));
        assert_eq!(slot.status().map(|status| status.name), Some("b.bib".into()));
        assert!(!slot.finish(a.generation, read(&a)), "A's late answer changes nothing");
        assert_eq!(slot.status().map(|status| status.name), Some("b.bib".into()));
        assert!(slot.next_read(a.generation).is_none(), "and A's reader takes no other work");
        assert!(slot.next_read(b.generation).is_none());
        assert!(slot.lock().readers.is_empty());

        // Quick switching keeps at most one replaced read beside the current
        // one; later choices wait, and only the latest of them is read.
        let c = slot.enqueue_read(slot.choose(Some("/invented/c.bib".into())).unwrap()).unwrap();
        let d = slot.enqueue_read(slot.choose(Some("/invented/d.bib".into())).unwrap()).unwrap();
        for name in ["e.bib", "f.bib"] {
            let queued = slot.choose(Some(std::path::Path::new("/invented").join(name))).unwrap();
            assert!(slot.enqueue_read(queued).is_none());
        }
        let f = slot.next_read(c.generation).unwrap_or_else(|| panic!("the latest choice"));
        assert_eq!(f.path.file_name().unwrap(), "f.bib");
        assert!(slot.next_read(d.generation).is_none());
        assert!(slot.next_read(f.generation).is_none());
        assert!(slot.lock().readers.is_empty());
        let g = slot.enqueue_read(slot.choose(Some("/invented/g.bib".into())).unwrap()).unwrap();
        slot.enqueue_read(slot.choose(Some("/invented/h.bib".into())).unwrap()).unwrap();
        let abandoned = slot.choose(Some("/invented/i.bib".into())).unwrap();
        assert!(slot.enqueue_read(abandoned).is_none());
        slot.choose(None);
        assert!(slot.next_read(g.generation).is_none(), "leaving a project drops its queued read");
    }

    #[test]
    fn context_validation_freezes_the_output_and_version_together() {
        let slot = ProjectBibliography::default();
        slot.lock().output = Some(crate::output::stored(Some("cite")));
        let (a, _) = slot
            .capture(Some(slot.version()), Some(crate::output::stored(Some("cite"))))
            .unwrap();
        slot.choose(None);
        slot.lock().output = Some(crate::output::stored(Some("typst")));
        assert!(
            slot.capture(Some(a), Some(crate::output::stored(Some("cite"))))
                .is_err()
        );
        assert!(
            slot.capture(Some(slot.version()), Some(crate::output::stored(Some("cite"))))
                .is_err()
        );
        let (engine, preview) = binding(None);
        assert!(slot.bind_preview(&engine, &preview, a).is_err());
        assert!(
            slot.commit::<()>("old-format", |_| panic!("old format cannot write in new project"))
                .is_err()
        );
        assert!(
            slot.capture(Some(slot.version()), Some(crate::output::stored(Some("typst"))))
                .is_ok()
        );
    }

    #[test]
    fn an_entry_copy_keeps_its_project_guard_until_the_clipboard_write_finishes() {
        let slot = ProjectBibliography::default();
        let generation = slot.version().0;
        assert_eq!(slot.commit_generation(generation, || "entry A"), Some("entry A"));
        slot.choose(None);
        assert_eq!(slot.commit_generation(generation, || "must not reach clipboard"), None);
    }

    #[test]
    fn a_new_bibliography_must_finish_reading_before_a_citation_can_be_prepared() {
        let (_dir, path) = written("paper.bib", A);
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(path)).unwrap();
        let output = Some(crate::output::stored(Some("pandoc")));
        assert!(
            slot.capture(None, output).is_err(),
            "an unowned request cannot prepare a citation"
        );
        assert!(slot.capture(Some(slot.version()), output).is_err());
        assert!(
            slot.capture(Some(slot.version()), Some(crate::output::stored(Some("apa"))))
                .is_ok(),
            "formatted text does not check bibliography keys"
        );
        assert!(
            slot.commit::<()>("not-prepared", |_| panic!("must not write before checking"))
                .is_err()
        );
        slot.finish(ticket.generation, read(&ticket));
        assert!(slot.capture(Some(slot.version()), output).is_ok());
        let (engine, preview) = binding(None);
        slot.bind_preview(&engine, &preview, slot.version()).unwrap();
        slot.commit(&preview.ticket, |current| {
            assert_eq!(current.unwrap().name, "paper.bib");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn rows_and_warnings_use_the_key_a_citation_would_emit() {
        use crate::library::engine_library::FileLibrary;
        use ibis_engine::Cancellation;
        let (_library_dir, library) = written(
            "refs.bib",
            "@book{a2026,title={A}}\n@book{gone2026,title={Gone}}\n@book{clash2026,title={Library title}}\n",
        );
        let (_project_dir, project) = written(
            "thesis.bib",
            "@book{a2026,title={A}}\n@book{clash2026,title={Other title}}\n",
        );
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(project)).unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.finish(ticket.generation, read(&ticket)));
        let current = slot.current().unwrap_or_else(|| panic!("a read"));

        let file =
            FileLibrary::open(&library, &ibis_engine::Cancellation::default()).unwrap_or_else(|_| panic!("a library"));
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
        assert_eq!(
            state("gone2026"),
            Some((serde_json::json!("missing"), generation.clone()))
        );
        assert_eq!(
            state("clash2026"),
            Some((serde_json::json!("may-name-another"), generation))
        );

        let selections = rows["results"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        serde_json::from_value(serde_json::json!({"id":row["id"],"cite":row["cite"]})).ok()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let preview = crate::test_citations::preview(&file.engine, file.sample, selections, "pandoc").unwrap();
        let mut warnings = crate::test_citations::said(
            &crate::project::preview_warnings(&file.engine, &preview.ticket, &preview.content_hash, Some(&current))
                .unwrap(),
        );
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
    fn copying_reports_what_the_preview_warned_about_for_key_outputs_only() {
        use crate::library::engine_library::FileLibrary;
        use ibis_engine::Cancellation;
        let (_library_dir, library) = written(
            "refs.bib",
            "@book{gone2026,title={Gone},author={Lee, Kim},year={2026}}\n",
        );
        let (_project_dir, project) = written("thesis.bib", A);
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(project)).unwrap_or_else(|| panic!("a ticket"));
        slot.finish(ticket.generation, read(&ticket));
        let current = slot.current();

        let file =
            FileLibrary::open(&library, &ibis_engine::Cancellation::default()).unwrap_or_else(|_| panic!("a library"));
        let query = serde_json::from_value(serde_json::json!({"text":"gone","interpret_locators":false}))
            .unwrap_or_else(|error| panic!("{error}"));
        let reply = file
            .search(&query, 10, None, &Cancellation::default())
            .unwrap_or_else(|error| panic!("{error}"));
        let rows = serde_json::to_value(&reply).unwrap_or_else(|error| panic!("{error}"));
        let copied = |style: Option<&str>| {
            let selection = serde_json::from_value(serde_json::json!({
                "id": rows["results"][0]["id"], "cite": rows["results"][0]["cite"], "locator": null
            }))
            .unwrap_or_else(|error| panic!("{error}"));
            let preview =
                crate::test_citations::preview(&file.engine, file.sample, vec![selection], style.unwrap_or("pandoc"))
                    .unwrap_or_else(|_| panic!("a preview"));
            let mut copied =
                crate::test_citations::copy(&file.engine, &preview.ticket, &preview.content_hash, |_| Ok(()))
                    .unwrap_or_else(|_| panic!("a copy"));
            let warnings = crate::project::preview_warnings(
                &file.engine,
                &preview.ticket,
                &preview.content_hash,
                current.as_ref(),
            )
            .unwrap();
            copied.warnings.extend(warnings);
            crate::test_citations::said(&copied.warnings)
        };
        assert_eq!(copied(None), ["gone2026 is missing from thesis.bib."]);
        assert_eq!(copied(Some("apa")), Vec::<String>::new());
    }

    /// One picker copy on the mock app, as `copy_file_citation` makes it with
    /// no editor waiting and the clipboard left out: the preview, then the commit.
    struct Picker {
        app: tauri::App<tauri::test::MockRuntime>,
        _library_dir: tempfile::TempDir,
    }

    impl Picker {
        fn new(library: &str, project: &std::path::Path) -> Self {
            use crate::library::engine_library::FileLibrary;
            let (library_dir, path) = written("refs.bib", library);
            let file =
                FileLibrary::open(&path, &ibis_engine::Cancellation::default()).unwrap_or_else(|_| panic!("a library"));
            let mut open = Library::default();
            open.file = Some(std::sync::Arc::new(file));
            let app = tauri::test::mock_builder()
                .manage(Mutex::new(open))
                .manage(crate::editor_endpoint::Pending::default())
                .manage(ProjectBibliography::default())
                .manage(crate::session_references::SessionReferences::default())
                .build(tauri::test::mock_context(tauri::test::noop_assets()))
                .unwrap_or_else(|error| panic!("{error}"));
            let slot = app.state::<ProjectBibliography>();
            let ticket = slot
                .choose(Some(project.to_owned()))
                .unwrap_or_else(|| panic!("a ticket"));
            assert!(slot.finish(ticket.generation, read(&ticket)));
            Self {
                app,
                _library_dir: library_dir,
            }
        }

        fn read_again(&self) {
            let slot = self.app.state::<ProjectBibliography>();
            let again = slot.check_again().unwrap_or_else(|| panic!("a ticket"));
            slot.finish(again.generation, read(&again));
        }

        fn prepare(
            &self,
            text: &str,
            request: u32,
        ) -> (
            std::sync::Arc<crate::library::engine_library::FileLibrary>,
            ibis_core::contract::Preview,
        ) {
            use ibis_engine::Cancellation;
            let file = self
                .app
                .state::<Mutex<Library>>()
                .lock()
                .ok()
                .and_then(|library| library.file.clone());
            let file = file.unwrap_or_else(|| panic!("a library"));
            let query = serde_json::from_value(serde_json::json!({"text":text,"interpret_locators":false}))
                .unwrap_or_else(|error| panic!("{error}"));
            let reply = file
                .search(&query, 10, None, &Cancellation::default())
                .unwrap_or_else(|error| panic!("{error}"));
            let rows = serde_json::to_value(&reply).unwrap_or_else(|error| panic!("{error}"));
            let selection = serde_json::from_value(serde_json::json!({
                "id": rows["results"][0]["id"], "cite": rows["results"][0]["cite"], "locator": null
            }))
            .unwrap_or_else(|error| panic!("{error}"));
            let preview = tauri::async_runtime::block_on(crate::copy::preview_file_citation(
                vec![selection],
                "pandoc".into(),
                String::new(),
                crate::library::PreviewIntent {
                    keep_order: false,
                    page: 1,
                    request,
                    cancel: false,
                    context: Some(self.app.state::<ProjectBibliography>().version()),
                },
                self.app.state(),
                self.app.state(),
                self.app.state(),
            ))
            .unwrap_or_else(|_| panic!("a preview"));
            (file, preview)
        }

        /// The preview's warnings, then the copy's, of the one row `text` finds.
        fn cite(&self, text: &str, request: u32) -> (Vec<CitedWarning>, crate::copy::Copied) {
            let (file, preview) = self.prepare(text, request);
            let copied = crate::copy::commit_copy(
                &file,
                &file
                    .engine
                    .citations()
                    .commit(
                        ibis_engine::citations::SessionId::PICKER,
                        &preview.ticket,
                        &preview.content_hash,
                    )
                    .unwrap_or_else(|_| panic!("recited")),
                crate::copy::ProjectCopy {
                    project: &self.app.state::<ProjectBibliography>(),
                    session: &self.app.state::<crate::session_references::SessionReferences>(),
                },
                |_| Ok(None),
            )
            .unwrap_or_else(|_| panic!("a copy"));
            (preview.warnings, copied)
        }

        fn listed(&self) -> Vec<ItemRef> {
            self.app.state::<crate::session_references::SessionReferences>().items()
        }
    }

    #[test]
    fn a_copy_warns_about_the_project_and_still_lists_the_reference_for_the_session() {
        let (_project_dir, project) = written("thesis.bib", A);
        let picker = Picker::new("@book{gone2026,title={Gone},author={Lee, Kim},year={2026}}\n", &project);

        let (previewed, copied) = picker.cite("gone", 1);
        assert_eq!(
            crate::test_citations::said(&previewed),
            ["gone2026 is missing from thesis.bib."]
        );
        assert_eq!(copied.warnings, previewed, "the copy says what the preview said");
        assert_eq!(copied.text, "[@gone2026]");
        assert_eq!(picker.listed(), copied.cited, "and the session list has the reference");
        assert_eq!(picker.listed().len(), 1);
    }

    #[test]
    fn a_proven_wrong_work_key_cannot_reach_the_clipboard() {
        let (_project_dir, path) = written("thesis.bib", "@book{a2026,title={Another work},doi={10.5555/other}}\n");
        let picker = Picker::new("@book{a2026,title={Intended work},doi={10.5555/intended}}\n", &path);
        let (file, preview) = picker.prepare("a2026", 1);
        assert!(preview.project_refusal.is_some());
        let mut writes = 0;
        let copied = crate::copy::commit_copy(
            &file,
            &file
                .engine
                .citations()
                .commit(
                    ibis_engine::citations::SessionId::PICKER,
                    &preview.ticket,
                    &preview.content_hash,
                )
                .unwrap_or_else(|_| panic!("recited")),
            crate::copy::ProjectCopy {
                project: &picker.app.state::<ProjectBibliography>(),
                session: &picker.app.state::<crate::session_references::SessionReferences>(),
            },
            |_| {
                writes += 1;
                Ok(None)
            },
        );
        assert!(copied.is_err());
        assert_eq!(writes, 0);
        assert!(picker.listed().is_empty());
    }

    #[test]
    fn a_key_citation_waits_for_the_check_that_reopening_starts() {
        let intended = "@book{a2026,title={Intended work},doi={10.5555/intended}}\n";
        let (_project_dir, path) = written("thesis.bib", intended);
        let picker = Picker::new(intended, &path);
        let (file, preview) = picker.prepare("a2026", 1);
        assert_eq!(preview.project_refusal, None, "the key names the intended work");

        // The file's key now names another work; the picker opens again and its
        // check is still reading when Enter is pressed.
        std::fs::write(&path, "@book{a2026,title={Another work},doi={10.5555/other}}\n")
            .unwrap_or_else(|error| panic!("{error}"));
        let slot = picker.app.state::<ProjectBibliography>();
        let again = slot.check_again().unwrap_or_else(|| panic!("a ticket"));
        assert_eq!(slot.status().map(|status| status.state), Some(ProjectState::Reading));
        assert!(
            slot.capture(Some(slot.version()), Some(crate::output::stored(Some("pandoc"))))
                .is_err(),
            "no key citation is prepared from the answer being replaced"
        );
        assert!(
            slot.capture(Some(slot.version()), Some(crate::output::stored(Some("apa"))))
                .is_ok(),
            "formatted text does not wait"
        );
        let mut writes = 0;
        let copied = crate::copy::commit_copy(
            &file,
            &file
                .engine
                .citations()
                .commit(
                    ibis_engine::citations::SessionId::PICKER,
                    &preview.ticket,
                    &preview.content_hash,
                )
                .unwrap_or_else(|_| panic!("recited")),
            crate::copy::ProjectCopy {
                project: &slot,
                session: &picker.app.state::<crate::session_references::SessionReferences>(),
            },
            |_| {
                writes += 1;
                Ok(None)
            },
        );
        assert!(copied.is_err());
        assert_eq!(writes, 0, "nothing reaches the clipboard or is pasted");
        assert!(picker.listed().is_empty());

        // The check's answer arrives, and the citation is refused for it.
        assert!(slot.finish(again.generation, read(&again)));
        let (_, preview) = picker.prepare("a2026", 2);
        assert!(preview.project_refusal.is_some());
    }

    #[test]
    fn an_unreadable_bibliography_is_a_warning_and_the_citation_is_still_copied() {
        let (_project_dir, project) = written("thesis.bib", "@book{a2026,title={A},author={Lee, Kim},year={2026}}\n");
        let picker = Picker::new("@book{a2026,title={A},author={Lee, Kim},year={2026}}\n", &project);
        let (previewed, copied) = picker.cite("a2026", 1);
        assert!(
            previewed.is_empty() && copied.warnings.is_empty(),
            "plainly in the file"
        );

        std::fs::remove_file(&project).unwrap_or_else(|error| panic!("{error}"));
        picker.read_again();
        let (file, preview) = picker.prepare("a2026", 2);
        assert_eq!(preview.project_refusal, None);
        assert_eq!(
            crate::test_citations::said(&preview.warnings),
            ["Couldn’t check a2026 against thesis.bib."]
        );
        let copied = crate::copy::commit_copy(
            &file,
            &file
                .engine
                .citations()
                .commit(
                    ibis_engine::citations::SessionId::PICKER,
                    &preview.ticket,
                    &preview.content_hash,
                )
                .unwrap_or_else(|_| panic!("recited")),
            crate::copy::ProjectCopy {
                project: &picker.app.state::<ProjectBibliography>(),
                session: &picker.app.state::<crate::session_references::SessionReferences>(),
            },
            |_| Ok(Some(17)),
        )
        .unwrap_or_else(|_| panic!("a copy"));
        assert_eq!(copied.text, "[@a2026]");
        assert_eq!(copied.delivery, Some(17));
        assert_eq!(copied.warnings, preview.warnings, "the copy says what the preview said");
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
    fn new_content_in_a_healthy_file_changes_the_status_so_rows_are_asked_again() {
        let (_dir, a) = written("a.bib", A);
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(a.clone())).unwrap_or_else(|| panic!("a ticket"));
        slot.finish(ticket.generation, read(&ticket));
        let before = slot.status();

        std::fs::write(&a, B).unwrap_or_else(|error| panic!("{error}"));
        let again = slot.check_again().unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.finish(again.generation, read(&again)));
        let after = slot.status();
        assert_eq!(
            before.as_ref().map(|status| status.state),
            after.as_ref().map(|status| status.state),
            "still ready"
        );
        assert_ne!(before, after, "yet not the same answer");

        // An unchanged read leaves the status alone, so it asks nobody anything.
        let unchanged = slot.check_again().unwrap_or_else(|| panic!("a ticket"));
        assert!(!slot.finish(unchanged.generation, read(&unchanged)));
        assert_eq!(slot.status(), after);
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
        assert!(
            !slot.finish(again.generation, read(&again)),
            "an unchanged file wakes nothing"
        );
        assert!(slot.check_again().is_some(), "and leaves no read running");
    }

    #[test]
    fn a_file_that_goes_away_checks_nothing() {
        let (_dir, a) = written("a.bib", A);
        let slot = ProjectBibliography::default();
        let ticket = slot.choose(Some(a.clone())).unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.finish(ticket.generation, read(&ticket)));
        assert_eq!(slot.status().map(|status| status.state), Some(ProjectState::Ready));

        std::fs::remove_file(&a).unwrap_or_else(|error| panic!("{error}"));
        let again = slot.check_again().unwrap_or_else(|| panic!("a ticket"));
        assert!(slot.finish(again.generation, read(&again)));
        assert_eq!(slot.status().map(|status| status.state), Some(ProjectState::Failed));
        let current = slot.current().unwrap_or_else(|| panic!("a read"));
        assert_eq!(current.read.check("a2026", None), KeyCheck::CouldNotCheck);
        assert_eq!(current.read.check("z2026", None), KeyCheck::CouldNotCheck);
    }
}
