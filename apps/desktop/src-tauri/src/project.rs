//! Native bibliography chooser and clipboard effects.
use crate::diagnostics::{NativeIssue, report};
use crate::library::commands::RecentIdentity;
use crate::library::engine_library::FileLibrary;
use crate::picker::ChooserOpen;
pub(crate) use ibis_engine::app_context::project::{Current, ProjectBibliography, preview_evidence, preview_warnings};
use ibis_engine::library::Owner;
use ibis_engine::{EntryRefusal, EntryResult};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

pub(crate) fn watch_openings(app: &AppHandle) {
    use tauri::Listener;
    let project = app.state::<ProjectBibliography>().inner().clone();
    app.listen("picker:shown", move |_| project.recheck());
}

/// Choose the file with the system dialog; the interface never handles a path.
#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn choose_project(app: AppHandle) {
    let Some(owner) = app.state::<ChooserOpen>().begin() else {
        return;
    };
    #[cfg(target_os = "macos")]
    crate::panel::activate_for_dialog(&app);
    let generation = app.state::<crate::picker::PickerGeneration>().current();
    let Ok(bibliography_owner) = crate::app_context::bibliography_owner(&app) else {
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
                && crate::app_context::set_bibliography(&handle, &bibliography_owner, Some(path)).is_err()
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
    library: State<'_, Owner>,
) -> Result<EntryCopy, crate::library::CommandError> {
    let file = library.current().ok_or(crate::library::CommandError::SnapshotChanged)?;
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
    crate::app_context::refresh(&app, captured.as_ref().map(|(id, _, _)| id))?;
    app.state::<ProjectBibliography>()
        .commit_generation(generation, || {
            crate::copy::write_plain(&app, &text).map_err(|_| crate::library::CommandError::ClipboardUnavailable)?;
            Ok(EntryCopy::Copied)
        })
        .unwrap_or(Ok(EntryCopy::Changed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::contract::{CitedWarning, ItemRef};
    use ibis_engine::app_context::output;
    use ibis_engine::app_context::project::ProjectState;
    use std::path::PathBuf;

    fn written(name: &str, text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        (dir, path)
    }

    fn wait_read(slot: &ProjectBibliography) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while slot
            .status()
            .is_some_and(|status| status.state == ProjectState::Reading)
        {
            assert!(std::time::Instant::now() < deadline, "bibliography read timed out");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn activate(slot: &ProjectBibliography, path: PathBuf) {
        slot.activate(
            "invented-writing".into(),
            Some(path),
            Some(output::stored(Some("pandoc"))),
        );
        wait_read(slot);
    }
    #[test]
    fn rows_and_warnings_use_the_key_a_citation_would_emit() {
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
        activate(&slot, project);
        let current = slot.current().unwrap_or_else(|| panic!("a read"));

        let file = ibis_engine::library::Current::open(
            ibis_engine::Engine::new()
                .unwrap_or_else(|_| panic!("engine"))
                .with_formatting(crate::library::engine_library::FORMATTING.clone()),
            ibis_engine::library::Source::File(library.clone()),
            None,
            &ibis_engine::Cancellation::default(),
        )
        .unwrap_or_else(|_| panic!("a library"));
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
        let generation = serde_json::json!(slot.version().0);
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
        let preview = crate::test_citations::preview(&file.engine, file.sample(), selections, "pandoc").unwrap();
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

    /// One picker copy on the mock app, as `deliver_citation` makes it with
    /// no editor waiting and the clipboard left out: the preview, then the commit.
    struct Picker {
        app: tauri::App<tauri::test::MockRuntime>,
        _library_dir: tempfile::TempDir,
    }

    impl Picker {
        fn new(library: &str, project: &std::path::Path) -> Self {
            let (library_dir, path) = written("refs.bib", library);
            let open = crate::library::tests::owner(crate::preferences::SettingsFile::new(
                library_dir.path().join("settings.json"),
            ));
            crate::library::tests::open(&open, path);
            let app = tauri::test::mock_builder()
                .manage(open)
                .manage(crate::editor_endpoint::Pending::default())
                .manage(ProjectBibliography::default())
                .manage(crate::session_references::SessionReferences::default())
                .build(tauri::test::mock_context(tauri::test::noop_assets()))
                .unwrap_or_else(|error| panic!("{error}"));
            let slot = app.state::<ProjectBibliography>();
            activate(&slot, project.to_owned());
            Self {
                app,
                _library_dir: library_dir,
            }
        }

        fn read_again(&self) {
            let slot = self.app.state::<ProjectBibliography>();
            slot.recheck();
            wait_read(&slot);
        }

        fn prepare(
            &self,
            text: &str,
            request: u32,
        ) -> (
            std::sync::Arc<ibis_engine::library::Current>,
            ibis_core::contract::Preview,
        ) {
            use ibis_engine::Cancellation;
            let file = self.app.state::<Owner>().current();
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
}
