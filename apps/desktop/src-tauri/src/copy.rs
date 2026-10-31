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
    #[specta(optional)]
    pub(crate) summary: Option<ReceiptSummary>,
    /// Unique write token for this copy's one-use paste proof.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(crate) delivery: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, specta::Type)]
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
) -> Result<crate::library::engine_file::FilePreview, CommandError> {
    let cancel = requests
        .start(intent)
        .ok_or_else(|| CommandError::from("A newer citation preview replaced this request."))?;
    let file = {
        let library = library
            .lock()
            .map_err(|_| CommandError::from("The library is unavailable."))?;
        let _ = expected_revision;
        if library.opening {
            return Err("The library is loading. Try again after it opens.".into());
        }
        library
            .file
            .clone()
            .ok_or_else(|| CommandError::from("The selected file is no longer open."))?
    };
    let reservation = file.reserve_preview_with(cancel);
    tauri::async_runtime::spawn_blocking(move || file.preview(reservation, items, format, style))
        .await
        .map_err(|_| CommandError::from("The preview is unavailable. Try again."))?
}
#[tauri::command]
#[specta::specta]
pub(crate) fn copy_file_citation(
    ticket: String,
    content_hash: String,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
) -> Result<Copied, CommandError> {
    // The library lock is held only to take the engine; the engine's own
    // commit barrier guards the write.
    let file = {
        let library = library.lock().map_err(|_| CommandError::SnapshotChanged)?;
        if library.opening {
            return Err("The library is loading. Try again after it opens.".into());
        }
        library.file.clone().ok_or(CommandError::SnapshotChanged)?
    };
    #[cfg(target_os = "windows")]
    let target = crate::paste::copy_target(&app);
    let mut delivery = None;
    let mut copied = file.copy(&ticket, &content_hash, |content| {
        #[cfg(target_os = "windows")]
        {
            let sequence = clipboard::write(&app, content)?;
            delivery = crate::paste::remember_copy(&app, target, sequence);
        }
        #[cfg(not(target_os = "windows"))]
        {
            delivery = clipboard::write(&app, content)?;
        }
        Ok(())
    })?;
    copied.delivery = delivery;
    Ok(copied)
}
