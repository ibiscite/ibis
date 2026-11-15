//! Copying the selected references as a citation.

use ibis_engine::citations::{Operation, SessionId};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::library::{CommandError, Library, PreviewIntent};

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct Copied {
    pub(crate) text: String,
    /// What the chosen syntax could not express, and other problems the user should see.
    pub(crate) warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<ReceiptSummary>,
    /// Unique clipboard-write identity for receipts, even without a native paste proof.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) delivery: Option<u32>,
    /// What was cited, for the session's list and the project check; stays
    /// on this side of the bridge.
    #[serde(skip)]
    pub(crate) cited: Vec<ibis_core::contract::ItemRef>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
pub(crate) struct ReceiptSummary {
    pub(crate) text: String,
    pub(crate) more: usize,
}

impl ReceiptSummary {
    pub(crate) fn citation(result: &ibis_core::contract::CitationResult) -> Self {
        const LIMIT: usize = 120;
        let line = result.text.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.chars().count() <= LIMIT {
            return Self { text: line, more: 0 };
        }
        let owners: std::collections::BTreeSet<_> = result.display.nodes.iter().filter_map(|node| node.item).collect();
        let first = if owners.len() <= 1 {
            line
        } else {
            let first = result.display.nodes.iter().find_map(|node| node.item);
            result
                .display
                .nodes
                .iter()
                .take_while(|node| node.item.is_none() || node.item == first)
                .flat_map(|node| &node.content)
                .map(|run| run.text.as_str())
                .collect::<String>()
        };
        let mut text: String = first
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(LIMIT - 1)
            .collect();
        text.push('…');
        Self {
            text,
            more: owners.len().saturating_sub(1),
        }
    }
}

#[cfg(test)]
mod shelf_summary_tests {
    use super::*;
    use ibis_core::contract::*;
    #[test]
    fn shelf_summary_keeps_narrative_remainders_and_counts_remaining_references() {
        let text = format!("Taylor (2026, p. {})", "12–30 ".repeat(40));
        let node = |item, text: String| DisplayNode {
            blocks: vec![],
            item,
            content: vec![DisplayRun {
                text,
                ..DisplayRun::default()
            }],
        };
        let result = CitationResult::new(
            text.clone(),
            None,
            None,
            DisplayTree {
                nodes: vec![
                    node(Some(0), "Taylor".into()),
                    node(None, " ".into()),
                    node(Some(0), text[7..].into()),
                ],
            },
            vec![ItemOutcome::Rendered { item: 0 }],
            vec![],
            vec![],
        );
        let summary = ReceiptSummary::citation(&result);
        assert!(summary.text.starts_with("Taylor (2026, p."));
        assert!(summary.text.ends_with('…'));
        assert_eq!(summary.more, 0);
        let result = CitationResult::new(
            "Long synthetic citation ".repeat(20),
            None,
            None,
            DisplayTree {
                nodes: (0..5)
                    .map(|item| node(Some(item), format!("Synthetic reference {item}; ")))
                    .collect(),
            },
            (0..5).map(|item| ItemOutcome::Rendered { item }).collect(),
            vec![],
            vec![],
        );
        assert_eq!(ReceiptSummary::citation(&result).more, 4);
    }
}

/// Allocate under the platform's write lock, before clipboard I/O. Exhaustion
/// refuses the write instead of wrapping or publishing an unidentified copy.
pub(crate) fn write_token() -> Result<u32, String> {
    let failed = || "The clipboard could not be written.".to_owned();
    let mut token = WRITES.lock().map_err(|_| failed())?;
    *token = token.checked_add(1).ok_or_else(failed)?;
    Ok(*token)
}

/// Every Ibis clipboard write takes the next token.
static WRITES: Mutex<u32> = Mutex::new(0);

/// Where the clipboard stands: Ibis's own writes, and the system's count of
/// writes by any app where it keeps one (macOS and Windows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClipboardMark {
    ours: u32,
    system: Option<i64>,
}

pub(crate) fn clipboard_mark() -> ClipboardMark {
    ClipboardMark {
        ours: *WRITES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        system: clipboard::sequence(),
    }
}

/// The picker and clipboard this request belongs to, captured before reciting.
#[derive(Clone, Copy)]
struct CopyIntent {
    presentation: u64,
    clipboard: ClipboardMark,
}

impl CopyIntent {
    fn write<T>(
        self,
        presentation: u64,
        clipboard: ClipboardMark,
        write: impl FnOnce() -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        if presentation != self.presentation || clipboard != self.clipboard {
            return Err(CommandError::Cancelled);
        }
        write()
    }
}

/// A copy's content, kept while it moves to the main thread to be written.
pub(crate) struct OwnedContent {
    text: String,
    html: Option<String>,
    rtf: Option<String>,
}

impl OwnedContent {
    pub(crate) fn of(content: ibis_core::contract::ClipboardContent<'_>) -> Self {
        Self {
            text: content.text.to_owned(),
            html: content.html.map(str::to_owned),
            rtf: content.rtf.map(str::to_owned),
        }
    }

    fn content(&self) -> ibis_core::contract::ClipboardContent<'_> {
        ibis_core::contract::ClipboardContent {
            text: &self.text,
            html: self.html.as_deref(),
            rtf: self.rtf.as_deref(),
        }
    }
}

/// On the main thread, where citations are written: copy `content` unless the
/// clipboard was written since `began`, here or in another app. That copy is
/// newer and stays; this one ends as cancelled.
pub(crate) fn write_copy_only_unless_written(
    app: &AppHandle,
    content: &OwnedContent,
    began: ClipboardMark,
) -> Result<(), CommandError> {
    if clipboard_mark() != began {
        return Err(CommandError::Cancelled);
    }
    write_copy_only(app, content.content()).map_err(|_| CommandError::ClipboardUnavailable)
}

mod clipboard;
#[cfg(test)]
mod ownership_tests;

/// Read-only examples owned by a prepared preview; never publishes a new ticket.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn citation_details(
    ticket: String,
    content_hash: String,
    item: usize,
    intent: PreviewIntent,
    library: State<'_, Mutex<Library>>,
) -> Result<ibis_core::contract::FormDetails, CommandError> {
    let file = library
        .lock()
        .map_err(|_| CommandError::SnapshotChanged)?
        .file
        .clone()
        .ok_or(CommandError::SnapshotChanged)?;
    let cancel = file
        .engine
        .citations()
        .start(SessionId::PICKER, Operation::Forms, intent)?;
    let task = file
        .engine
        .citations()
        .forms(SessionId::PICKER, &ticket, &content_hash, item, cancel)?;
    tauri::async_runtime::spawn_blocking(move || task.run().map_err(CommandError::from))
        .await
        .map_err(|_| CommandError::NotPrepared)?
}

// Each managed state is its own argument: Tauri hands them to commands that way.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn preview_file_citation(
    items: Vec<ibis_core::contract::DraftItem>,
    output: String,
    expected_revision: String,
    intent: PreviewIntent,
    library: State<'_, Mutex<Library>>,
    editor: State<'_, crate::editor_endpoint::Pending>,
    project: State<'_, crate::project::ProjectBibliography>,
) -> Result<ibis_core::contract::Preview, CommandError> {
    let expected_context = intent.context;
    let keep_order = intent.keep_order;
    let (file, generation) = {
        let library = library.lock().map_err(|_| CommandError::SourceNotOpened)?;
        let _ = expected_revision;
        if library.opening {
            return Err(CommandError::LibraryLoading);
        }
        let file = library.file.clone().ok_or(CommandError::SnapshotChanged)?;
        (file, library.source_generation)
    };
    let cancel = file
        .engine
        .citations()
        .start(SessionId::PICKER, Operation::Preview, intent)?;
    // The engine's id, as the menu sent it; one it does not list is refused, not guessed.
    let output = crate::output::listed(&output)
        .ok_or_else(|| CommandError::from(ibis_core::contract::CitationError::UnsupportedOutput))?;
    let editor = editor.current();
    let (context_version, current_project) = project.capture(expected_context, editor.is_none().then_some(output))?;
    let output = output.spec().clone();
    // Only a key output depends on what the project's file holds. A waiting
    // editor names its own output.
    let key_output = matches!(
        editor.as_ref().map_or(&output, |request| &request.output),
        ibis_core::contract::OutputSpec::Key { .. }
    );
    let current_project = editor
        .as_ref()
        .and_then(|request| request.bibliography())
        .or(current_project)
        .filter(|_| key_output);
    let draft = ibis_core::contract::Draft {
        items,
        output: editor.as_ref().map_or(output, |editor| editor.output.clone()),
        form: editor
            .as_ref()
            .map_or(ibis_core::contract::CitationForm::Normal, |editor| editor.form),
        keep_order,
        input_language: None,
        output_locale: None,
        sample: file.sample,
    };
    let task = file.engine.citations().preview(SessionId::PICKER, draft, cancel)?;
    let engine = file.engine.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut preview = task.run()?;
        preview.warnings.extend(crate::project::preview_warnings(
            &file.engine,
            &preview.ticket,
            &preview.content_hash,
            current_project.as_ref(),
        )?);
        // Only a key known to name a different work is refused; the rest of
        // the check joined the warnings above.
        preview.project_refusal = crate::project::preview_evidence(
            &file.engine,
            &preview.ticket,
            &preview.content_hash,
            current_project.as_ref(),
        )?
        .filter(|assessment| !assessment.can_copy())
        .and_then(|assessment| assessment.message);
        Ok::<_, CommandError>(preview)
    })
    .await
    .map_err(|_| CommandError::NotPrepared)?;
    // A reload cancels previews it overtakes, and the quiet check that follows
    // each show retires Zotero while it reads. Both publish when they end, which
    // asks again; that is waiting, not a failure.
    let overtaken = result.is_err()
        && library
            .lock()
            .is_ok_and(|library| library.opening || library.checking || library.source_generation != generation);
    if overtaken {
        Err(CommandError::LibraryLoading)
    } else {
        let preview = result?;
        project.bind_preview(&engine, &preview, context_version)?;
        Ok(preview)
    }
}
/// Plain text with no delivery.
pub(crate) fn write_plain(app: &AppHandle, text: &str) -> Result<(), String> {
    write_copy_only(
        app,
        ibis_core::contract::ClipboardContent {
            text,
            html: None,
            rtf: None,
        },
    )
}

/// Copy the shown citation. Citing it again can wait for the formatter, so
/// that runs on a worker with no lock held; the short commit that writes the
/// clipboard stays on the main thread, as the native clipboard needs.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn copy_file_citation(
    ticket: String,
    content_hash: String,
    replaces: Vec<String>,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
) -> Result<Copied, CommandError> {
    let intent = CopyIntent {
        presentation: tauri::Manager::state::<crate::picker::PickerGeneration>(&app).current(),
        clipboard: clipboard_mark(),
    };
    // What the copy is for is fixed when it is asked for: a Forget or a new
    // target while it waits must not adopt it.
    let editor = tauri::Manager::state::<crate::editor_endpoint::Pending>(&app);
    let captured = editor.app(crate::paste::last_app(&app));
    let epoch = tauri::Manager::state::<crate::session_references::SessionReferences>(&app)
        .lock()
        .epoch();
    let file = open_file(&library)?;
    let recited = tauri::async_runtime::spawn_blocking(move || {
        file.engine
            .citations()
            .commit(SessionId::PICKER, &ticket, &content_hash)
            .map_err(CommandError::from)
    })
    .await
    .map_err(|_| CommandError::ClipboardUnavailable)??;
    let (send, committed) = std::sync::mpsc::sync_channel(1);
    let main = app.clone();
    app.run_on_main_thread(move || {
        let _ = send.send(commit_shown(&main, &recited, replaces, captured, epoch, intent));
    })
    .map_err(|_| CommandError::ClipboardUnavailable)?;
    tauri::async_runtime::spawn_blocking(move || committed.recv())
        .await
        .map_err(|_| CommandError::ClipboardUnavailable)?
        .map_err(|_| CommandError::ClipboardUnavailable)?
}

fn commit_shown(
    app: &AppHandle,
    recited: &ibis_engine::citations::CheckedCitation,
    replaces: Vec<String>,
    captured: Option<(ibis_core::destination::AppId, String, Option<String>)>,
    epoch: std::sync::Arc<()>,
    intent: CopyIntent,
) -> Result<Copied, CommandError> {
    use tauri::Manager;
    let library = app.state::<Mutex<Library>>();
    let editor = app.state::<crate::editor_endpoint::Pending>();
    let project = app.state::<crate::project::ProjectBibliography>();
    let app = app.clone();
    let session = tauri::Manager::state::<crate::session_references::SessionReferences>(&app);
    if editor.current().is_none() {
        // The citation uses the project as saved: a change made elsewhere since
        // the preview, by the command line for one, retires it before the commit.
        crate::writings::refresh(&app, captured.as_ref().map(|(id, _, _)| id))?;
    }
    #[cfg(target_os = "windows")]
    let attempt = crate::paste::begin_copy(&app, &replaces);
    let file = open_file(&library)?;
    let captured_id = captured.as_ref().map(|(id, _, _)| id.normalized());
    let record = |copied: &Copied| {
        if !file.sample {
            session.record(&epoch, captured_id.as_deref(), &copied.cited);
        }
    };
    // Serialize clipboard writes through recording; privacy invalidation has its own lock.
    static RECORDING: Mutex<()> = Mutex::new(());
    let _recording = RECORDING.lock().map_err(|_| CommandError::SnapshotChanged)?;
    // A waiting editor gets the citation in place of the clipboard. It is
    // handed over under the same commit barrier, and nothing is pasted.
    if let Some(request) = editor.current() {
        if recited.citation().request_output() != &request.output || recited.citation().request_form() != request.form {
            return Err(CommandError::SnapshotChanged);
        }
        let current = request.bibliography();
        if let Some(refused) =
            crate::project::preview_evidence(&file.engine, recited.ticket(), recited.content_hash(), current.as_ref())?
                .filter(|assessment| !assessment.can_copy())
        {
            return Err(CommandError::ProjectConflict {
                keys: refused.conflicts,
            });
        }
        let warnings =
            crate::project::preview_warnings(&file.engine, recited.ticket(), recited.content_hash(), current.as_ref())?;
        // The text leaves the commit barrier first; the editor is answered with no
        // lock held, because ending a request takes these locks the other way round.
        let mut text = None;
        let mut copied = copy_checked(&file.engine, recited, |content| {
            text = Some((
                content.text.to_owned(),
                content.html.map(str::to_owned),
                content.rtf.map(str::to_owned),
            ));
            Ok(())
        })?;
        copied.warnings.extend(warnings);
        let (text, html, rtf) = text.ok_or(CommandError::SnapshotChanged)?;
        if !editor.answer(request.id, &text, || {
            file.engine
                .citations()
                .invalidate(ibis_engine::citations::SessionId::PICKER)
        }) {
            return Err(CommandError::Cancelled);
        }
        // Only a request that was answered may leave its citation on the clipboard.
        if request.clipboard {
            intent.write(
                app.state::<crate::picker::PickerGeneration>().current(),
                clipboard_mark(),
                || {
                    write_copy_only(
                        &app,
                        ibis_core::contract::ClipboardContent {
                            text: &text,
                            html: html.as_deref(),
                            rtf: rtf.as_deref(),
                        },
                    )
                    .map_err(|_| CommandError::ClipboardUnavailable)
                },
            )?;
            session.lock().add(copied.cited.clone());
            record(&copied);
        }
        if matches!(request.output, ibis_core::contract::OutputSpec::Key { .. }) {
            app.state::<crate::preferences::SettingsFile>().record_key_format_use();
        }
        return Ok(copied);
    }
    let format = {
        let settings = ibis_engine::settings::load(&app.state::<crate::preferences::SettingsFile>().path);
        let target = captured.as_ref().map(|(id, _, _)| id);
        settings.cites().ok().and_then(|saved| {
            let chosen = ibis_engine::format::choose(saved, target);
            (chosen.output == *recited.citation().request_output()).then(|| (target.cloned(), chosen))
        })
    };
    let copied = commit_copy(
        &file,
        recited,
        ProjectCopy {
            project: &project,
            session: &session,
        },
        |content| {
            intent.write(
                app.state::<crate::picker::PickerGeneration>().current(),
                clipboard_mark(),
                || {
                    #[cfg(target_os = "windows")]
                    let delivery = {
                        let (token, sequence) =
                            clipboard::write(&app, content).map_err(|_| CommandError::ClipboardUnavailable)?;
                        crate::paste::remember_copy(&app, attempt, sequence, token)
                    };
                    #[cfg(not(target_os = "windows"))]
                    let delivery =
                        clipboard::write(&app, content, &replaces).map_err(|_| CommandError::ClipboardUnavailable)?;
                    Ok(delivery)
                },
            )
        },
    )?;
    record(&copied);
    if matches!(
        recited.citation().request_output(),
        ibis_core::contract::OutputSpec::Key { .. }
    ) {
        app.state::<crate::preferences::SettingsFile>().record_key_format_use();
    }
    if let Some((id, chosen)) = format {
        app.state::<crate::notice::Notice>()
            .remember_format(copied.delivery, id, chosen);
    }
    Ok(copied)
}

/// The open library's engine. The lock is held only to take it; the engine's
/// own commit barrier guards the write.
fn open_file(
    library: &Mutex<Library>,
) -> Result<std::sync::Arc<crate::library::engine_library::FileLibrary>, CommandError> {
    let library = library.lock().map_err(|_| CommandError::SnapshotChanged)?;
    if library.opening {
        return Err(CommandError::LibraryLoading);
    }
    library.file.clone().ok_or(CommandError::SnapshotChanged)
}

/// Commit the shown citation through `write`, which returns its delivery,
/// then say what the copy needs said and remember what it cited.
pub(crate) struct ProjectCopy<'a> {
    pub(crate) project: &'a crate::project::ProjectBibliography,
    pub(crate) session: &'a crate::session_references::SessionReferences,
}

pub(crate) fn commit_copy(
    file: &crate::library::engine_library::FileLibrary,
    recited: &ibis_engine::citations::CheckedCitation,
    context: ProjectCopy<'_>,
    write: impl FnOnce(ibis_core::contract::ClipboardContent<'_>) -> Result<Option<u32>, CommandError>,
) -> Result<Copied, CommandError> {
    let (ticket, content_hash) = (recited.ticket(), recited.content_hash());
    context.project.commit(ticket, |current| {
        let refused = crate::project::preview_evidence(&file.engine, ticket, content_hash, current.as_ref())?
            .filter(|assessment| !assessment.can_copy());
        if let Some(assessment) = refused {
            return Err(CommandError::ProjectConflict {
                keys: assessment.conflicts,
            });
        }
        let warnings = crate::project::preview_warnings(&file.engine, ticket, content_hash, current.as_ref())?;
        let mut delivery = None;
        let mut copied = copy_checked(&file.engine, recited, |content| {
            delivery = write(content)?;
            Ok(())
        })?;
        copied.delivery = delivery;
        // Project warnings keep the picker open like the citation's own.
        copied.warnings.extend(warnings);
        context.session.lock().add(copied.cited.clone());
        Ok(copied)
    })
}

/// Whole content with no delivery: it is copied, never pasted.
pub(crate) fn write_copy_only(
    app: &AppHandle,
    content: ibis_core::contract::ClipboardContent<'_>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let written = clipboard::write(app, content);
    // Nothing is replaced: this is a new copy, not a change to the last citation.
    #[cfg(not(target_os = "windows"))]
    let written = clipboard::write(app, content, &[]);
    written.map(|_| ())
}

/// Native delivery owns the side effect; the engine guard prevents stale content.
pub(crate) fn copy_checked(
    engine: &ibis_engine::Engine,
    checked: &ibis_engine::citations::CheckedCitation,
    write: impl FnOnce(ibis_core::contract::ClipboardContent<'_>) -> Result<(), CommandError>,
) -> Result<Copied, CommandError> {
    let guard = checked.check(engine)?;
    write(guard.content())?;
    let result = checked.result();
    Ok(Copied {
        summary: Some(ReceiptSummary::citation(result)),
        text: result.text.clone(),
        warnings: checked.warnings(),
        delivery: None,
        cited: checked.citation().items(),
    })
}
