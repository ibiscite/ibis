//! Settings confirms engine proposals by id; no file or library path crosses IPC.
use ibis_engine::write::{Proposals, Status, WriteError, WriteProblem};
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct WaitingProposal {
    id: String,
    source_app: String,
    library: String,
    summary: String,
    created_ms: u64,
    lines: Vec<String>,
    before: Option<String>,
    after: Option<String>,
    refusal: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub(crate) enum ProposalResult {
    Applied { backup: String },
    Rejected,
    Refused { sentence: String },
}

// Acquire before scheduling blocking work, including reads after a reopen.
static OWNER: tauri::async_runtime::Mutex<()> = tauri::async_runtime::Mutex::const_new(());

fn store(app: &tauri::AppHandle) -> Proposals {
    Proposals::beside(&app.state::<crate::preferences::SettingsFile>().path)
}

/// Waiting proposals and their engine previews, with library names only.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn waiting_proposals(app: tauri::AppHandle) -> Result<Vec<WaitingProposal>, String> {
    crate::cli_install::with_owner(&OWNER, move || waiting(&store(&app))).await
}

fn waiting(store: &Proposals) -> Result<Vec<WaitingProposal>, String> {
    store
        .list()
        .map_err(|error| error.sentence())?
        .into_iter()
        .filter(|proposal| proposal.status == Status::Pending)
        .map(|proposal| {
            let preview = store.show(&proposal.id).map_err(|error| error.sentence())?;
            Ok(WaitingProposal {
                id: proposal.id,
                source_app: proposal.source.client,
                library: preview.library,
                summary: preview.summary,
                created_ms: proposal.created_ms,
                lines: preview.lines,
                before: preview.before,
                after: preview.after,
                refusal: preview.refusal.map(|problem| WriteError::from(problem).sentence()),
            })
        })
        .collect()
}

/// Apply only after the person presses Apply on the displayed proposal.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn apply_proposal(app: tauri::AppHandle, id: String) -> Result<ProposalResult, String> {
    crate::cli_install::with_owner(&OWNER, move || {
        let file = app.state::<ibis_engine::library::Owner>().current();
        Ok(apply(&store(&app), file.as_ref().map(|file| &file.engine), &id))
    })
    .await
}

fn apply(store: &Proposals, engine: Option<&ibis_engine::Engine>, id: &str) -> ProposalResult {
    let result = store.get(id).and_then(|_| {
        engine
            .ok_or_else(|| WriteError::from(WriteProblem::NotOpen))?
            .apply_proposal(store, id)
    });
    match result {
        Ok(applied) => ProposalResult::Applied {
            backup: applied
                .backup
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        },
        Err(error) => ProposalResult::Refused {
            sentence: error.sentence(),
        },
    }
}

/// Reject a waiting proposal; no library changes.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn reject_proposal(app: tauri::AppHandle, id: String) -> Result<ProposalResult, String> {
    crate::cli_install::with_owner(&OWNER, move || Ok(reject(&store(&app), &id))).await
}

fn reject(store: &Proposals, id: &str) -> ProposalResult {
    match store.reject(id) {
        Ok(_) => ProposalResult::Rejected,
        Err(error) => ProposalResult::Refused {
            sentence: error.sentence(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposal_preview_and_receipt_expose_names_only_and_list_never_writes() {
        use ibis_engine::write::{ProposeRequest, Source, Via};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("signals.bib");
        let before = "@book{vale2026, title = {Invented signals}}\n";
        std::fs::write(&path, before).unwrap();
        let path = std::fs::canonicalize(&path).unwrap();
        let engine = ibis_engine::Engine::new().unwrap().without_file_watches();
        engine
            .open_file_observed(&path, &ibis_engine::Cancellation::default())
            .unwrap();
        let store = Proposals::beside(&dir.path().join("settings.json"));
        let proposal = engine
            .propose(
                &store,
                Source {
                    via: Via::Mcp,
                    client: "Invented AI app".into(),
                },
                ProposeRequest::Add {
                    library: None,
                    fields: serde_json::from_value(serde_json::json!({"type":"book", "title":"River maps"})).unwrap(),
                },
            )
            .unwrap();
        let shown = waiting(&store).unwrap();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].library, "signals.bib");
        assert_eq!(shown[0].source_app, "Invented AI app");
        assert!(shown[0].after.as_ref().unwrap().contains("River maps"));
        let wire = serde_json::to_string(&shown).unwrap();
        assert!(!wire.contains(&dir.path().to_string_lossy().to_string()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        let result = apply(&store, Some(&engine), &proposal.id);
        assert_eq!(
            result,
            ProposalResult::Applied {
                backup: "signals.bib.ibis-backup-1".into()
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("signals.bib.ibis-backup-1")).unwrap(),
            before
        );
        assert!(waiting(&store).unwrap().is_empty());
        assert_eq!(
            reject(&store, &proposal.id),
            ProposalResult::Refused {
                sentence: WriteError::from(WriteProblem::Decided).sentence()
            }
        );
    }

    #[test]
    fn proposal_unknown_id_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = Proposals::beside(&dir.path().join("settings.json"));
        let refused = ProposalResult::Refused {
            sentence: WriteError::from(WriteProblem::UnknownProposal).sentence(),
        };
        for id in ["0123456789abcdef", "../settings.json"] {
            assert_eq!(apply(&store, None, id), refused);
        }
        assert!(!store.dir().exists());
        for id in ["0123456789abcdef", "../settings.json"] {
            assert_eq!(reject(&store, id), refused);
        }
    }
}
