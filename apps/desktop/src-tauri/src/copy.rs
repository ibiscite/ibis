//! Copying the selected references as a citation.

use std::sync::Mutex;

use ibis_core::cite::KeyFormat;
use ibis_core::settings;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::library::{CommandError, Library, PreviewIntent, PreviewRequests};
use crate::preferences::SettingsFile;

#[derive(Serialize, specta::Type)]
pub(crate) struct Copied {
    pub(crate) text: String,
    /// What the chosen syntax could not express, and other problems the user should see.
    pub(crate) warnings: Vec<String>,
    /// Only an evidence-policy notice remains; other warnings still keep the picker open.
    #[specta(optional)]
    pub(crate) project_warning_only: Option<bool>,
    #[specta(optional)]
    pub(crate) summary: Option<ReceiptSummary>,
    /// Unique clipboard-write identity for receipts, even without a native paste proof.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(crate) delivery: Option<u32>,
    /// What was cited, for the session's list and the project check; stays
    /// on this side of the bridge.
    #[serde(skip)]
    pub(crate) cited: Vec<ibis_core::contract::ItemRef>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, specta::Type)]
pub(crate) struct ReceiptSummary {
    pub(crate) text: String,
    pub(crate) more: usize,
    #[specta(optional)]
    pub(crate) project_warning: Option<String>,
    #[specta(optional)]
    pub(crate) project_checked_at_ms: Option<u64>,
}

impl ReceiptSummary {
    pub(crate) fn citation(result: &ibis_core::contract::CitationResult) -> Self {
        const LIMIT: usize = 120;
        let line = result.text.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.chars().count() <= LIMIT {
            return Self {
                text: line,
                more: 0,
                project_warning: None,
                project_checked_at_ms: None,
            };
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
            project_warning: None,
            project_checked_at_ms: None,
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
    static NEXT: Mutex<u32> = Mutex::new(0);
    let failed = || "The clipboard could not be written.".to_owned();
    let mut token = NEXT.lock().map_err(|_| failed())?;
    *token = token.checked_add(1).ok_or_else(failed)?;
    Ok(*token)
}

/// The format the picker starts with: the last one chosen, or Pandoc.
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_format(settings_file: State<'_, SettingsFile>) -> KeyFormat {
    settings::load(&settings_file.path)
        .settings
        .copy_format
        .unwrap_or(KeyFormat::Pandoc)
}

mod clipboard;
#[cfg(test)]
mod engine_file_acceptance;

/// Read-only examples owned by a prepared preview; never publishes a new ticket.
#[tauri::command]
#[specta::specta]
pub(crate) async fn citation_details(
    ticket: String,
    content_hash: String,
    item: usize,
    library: State<'_, Mutex<Library>>,
) -> Result<crate::library::engine_file::ReferenceDetails, CommandError> {
    let file = library
        .lock()
        .map_err(|_| CommandError::SnapshotChanged)?
        .file
        .clone()
        .ok_or(CommandError::SnapshotChanged)?;
    tauri::async_runtime::spawn_blocking(move || file.details(&ticket, &content_hash, item))
        .await
        .map_err(|_| CommandError::from("Citation examples are unavailable. Close and reopen the menu to try again."))?
}

// Each managed state is its own argument: Tauri hands them to commands that way.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub(crate) async fn preview_file_citation(
    items: Vec<crate::library::engine_file::FileSelection>,
    format: KeyFormat,
    style: Option<settings::CitationStyle>,
    expected_revision: String,
    intent: PreviewIntent,
    library: State<'_, Mutex<Library>>,
    requests: State<'_, PreviewRequests>,
    editor: State<'_, crate::editor_endpoint::Pending>,
    project: State<'_, crate::project::ProjectBibliography>,
) -> Result<crate::library::engine_file::FilePreview, CommandError> {
    let expected_context = intent.context;
    let cancel = requests.start(intent).ok_or(CommandError::Cancelled)?;
    let (file, generation) = {
        let library = library
            .lock()
            .map_err(|_| CommandError::from("The library is unavailable."))?;
        let _ = expected_revision;
        if library.opening {
            return Err(CommandError::LibraryLoading);
        }
        let file = library
            .file
            .clone()
            .ok_or_else(|| CommandError::from("The selected file is no longer open."))?;
        (file, library.source_generation)
    };
    let reservation = file.reserve_preview_with(cancel);
    let editor = editor.current();
    let (context_version, current_project) = project.capture(
        expected_context,
        editor.is_none().then(|| {
            style.map_or(
                crate::destination::AppOutput::Key(format),
                crate::destination::AppOutput::Style,
            )
        }),
    )?;
    // Only a key output depends on what the project's file holds. A waiting
    // editor names its own output.
    let key_output = editor.as_ref().map_or(style.is_none(), |request| {
        matches!(request.output, ibis_core::contract::OutputSpec::Key { .. })
    });
    let current_project = current_project.filter(|_| key_output);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut preview = file.preview_for(reservation, items, format, style, editor.as_ref())?;
        preview.warnings.extend(file.prepared_project_warnings(
            &preview.ticket,
            &preview.content_hash,
            current_project.as_ref(),
        )?);
        if let Some(assessment) = file.prepared_project_evidence(
            &preview.ticket,
            &preview.content_hash,
            current_project.as_ref(),
            ibis_engine::ProjectPolicy::Normal,
        )? {
            preview.project_check = match assessment.state {
                ibis_engine::ProjectAssessmentState::Paused => Some(crate::library::engine_file::ProjectCheck::Paused),
                ibis_engine::ProjectAssessmentState::Conflict => {
                    Some(crate::library::engine_file::ProjectCheck::Conflict)
                }
                ibis_engine::ProjectAssessmentState::Retained => {
                    Some(crate::library::engine_file::ProjectCheck::Retained)
                }
                _ => None,
            };
            preview.project_message = assessment.message;
            preview.project_checked_at_ms = assessment.last_checked_ms;
            preview.project_unverified_allowed = Some(
                file.prepared_project_evidence(
                    &preview.ticket,
                    &preview.content_hash,
                    current_project.as_ref(),
                    ibis_engine::ProjectPolicy::Unchecked,
                )?
                .is_some_and(|assessment| assessment.can_copy()),
            );
        }
        Ok::<_, CommandError>(preview)
    })
    .await
    .map_err(|_| CommandError::from("The preview is unavailable. Try again."))?;
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
        project.bind_preview(reservation, &preview.ticket, context_version)?;
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

// Each managed state is its own argument: Tauri hands them to commands that way.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_file_citation(
    ticket: String,
    content_hash: String,
    replaces: Option<String>,
    unverified_project: Option<bool>,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
    editor: State<'_, crate::editor_endpoint::Pending>,
    project: State<'_, crate::project::ProjectBibliography>,
) -> Result<Copied, CommandError> {
    if editor.current().is_none() {
        // Another client may have edited this definition while the picker was
        // open. Observe completed edits before granting a clipboard commit.
        let captured = crate::paste::last_app(&app);
        crate::writing_projects::refresh(&app, captured.as_ref().map(|(id, _, _)| id)).map_err(CommandError::from)?;
    }
    #[cfg(target_os = "windows")]
    let attempt = crate::paste::begin_copy(&app, replaces.as_deref());
    let file = open_file(&library)?;
    // A waiting editor gets the citation in place of the clipboard. It is
    // handed over under the same commit barrier, and nothing is pasted.
    if let Some(request) = editor.current() {
        if !file.prepared_for(&ticket, &request) {
            return Err(CommandError::SnapshotChanged);
        }
        // The text leaves the commit barrier first; the editor is answered with no
        // lock held, because ending a request takes these locks the other way round.
        let mut text = None;
        let copied = file.copy(&ticket, &content_hash, |content| {
            text = Some((
                content.text.to_owned(),
                content.html.map(str::to_owned),
                content.rtf.map(str::to_owned),
            ));
            Ok(())
        })?;
        let (text, html, rtf) = text.ok_or(CommandError::SnapshotChanged)?;
        if !editor.answer(request.id, &text, || file.invalidate_preview()) {
            return Err("Your editor stopped waiting for this citation.".into());
        }
        // Only a request that was answered may leave its citation on the clipboard.
        if request.clipboard {
            write_copy_only(
                &app,
                ibis_core::contract::ClipboardContent {
                    text: &text,
                    html: html.as_deref(),
                    rtf: rtf.as_deref(),
                },
            )
            .map_err(CommandError::from)?;
        }
        return Ok(copied);
    }
    let session = tauri::Manager::state::<crate::session_references::SessionReferences>(&app);
    commit_copy(
        &file,
        &ticket,
        &content_hash,
        ProjectCopy {
            project: &project,
            session: &session,
            unverified: unverified_project.unwrap_or(false),
        },
        |content| {
            #[cfg(target_os = "windows")]
            let delivery = {
                let (token, sequence) = clipboard::write(&app, content)?;
                crate::paste::remember_copy(&app, attempt, sequence, token)
            };
            #[cfg(not(target_os = "windows"))]
            let delivery = clipboard::write(&app, content, replaces.as_deref())?;
            Ok(delivery)
        },
    )
}

/// The open library's engine. The lock is held only to take it; the engine's
/// own commit barrier guards the write.
fn open_file(
    library: &Mutex<Library>,
) -> Result<std::sync::Arc<crate::library::engine_file::FileLibrary>, CommandError> {
    let library = library.lock().map_err(|_| CommandError::SnapshotChanged)?;
    if library.opening {
        return Err("The library is loading. Try again after it opens.".into());
    }
    library.file.clone().ok_or(CommandError::SnapshotChanged)
}

/// Commit the shown citation through `write`, which returns its delivery,
/// then say what the copy needs said and remember what it cited.
pub(crate) struct ProjectCopy<'a> {
    pub(crate) project: &'a crate::project::ProjectBibliography,
    pub(crate) session: &'a crate::session_references::SessionReferences,
    pub(crate) unverified: bool,
}

pub(crate) fn commit_copy(
    file: &crate::library::engine_file::FileLibrary,
    ticket: &str,
    content_hash: &str,
    context: ProjectCopy<'_>,
    write: impl FnOnce(ibis_core::contract::ClipboardContent<'_>) -> Result<Option<u32>, String>,
) -> Result<Copied, CommandError> {
    context.project.commit(ticket, |current| {
        let policy = if context.unverified {
            ibis_engine::ProjectPolicy::Unchecked
        } else {
            ibis_engine::ProjectPolicy::Normal
        };
        let assessment = file.prepared_project_evidence(ticket, content_hash, current.as_ref(), policy)?;
        if let Some(assessment) = assessment.as_ref().filter(|assessment| !assessment.can_copy()) {
            return Err(assessment
                .message
                .clone()
                .unwrap_or_else(|| "Couldn’t check this citation against the project bibliography.".into())
                .into());
        }
        let mut warnings = file.prepared_project_warnings(ticket, content_hash, current.as_ref())?;
        if let Some(warning) = assessment
            .as_ref()
            .filter(|assessment| assessment.state == ibis_engine::ProjectAssessmentState::Unverified)
            .and_then(|assessment| assessment.warning())
        {
            warnings.push(warning);
        }
        let mut delivery = None;
        let mut copied = file.copy(ticket, content_hash, |content| {
            delivery = write(content)?;
            Ok(())
        })?;
        copied.delivery = delivery;
        if let Some(assessment) = assessment.filter(|assessment| {
            matches!(
                assessment.state,
                ibis_engine::ProjectAssessmentState::Retained | ibis_engine::ProjectAssessmentState::Unverified
            )
        }) {
            copied.project_warning_only = Some(copied.warnings.is_empty());
            if let Some(summary) = copied.summary.as_mut() {
                summary.project_warning = Some(
                    if assessment.state == ibis_engine::ProjectAssessmentState::Retained {
                        "Using the last checked bibliography copy."
                    } else {
                        "Unverified project key."
                    }
                    .to_owned(),
                );
                summary.project_checked_at_ms = assessment.last_checked_ms;
            }
        }
        // Independent warnings still keep the picker open. Evidence-policy
        // notices preserve the intended delivery and are repeated in its receipt.
        copied.warnings.extend(warnings);
        context.session.cited(copied.cited.clone());
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
    let written = clipboard::write(app, content, None);
    written.map(|_| ())
}
