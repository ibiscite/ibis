//! Settings confirms engine proposals by id; no file or library path crosses IPC.
use ibis_core::contract::Problem;
use ibis_engine::write::{Proposals, WriteError, WriteProblem};
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct WaitingProposal {
    id: String,
    source_app: String,
    library: String,
    summary: ibis_core::contract::Message,
    created_ms: u64,
    lines: Vec<String>,
    before: Option<String>,
    after: Option<String>,
    refusal: Option<Problem>,
}

#[derive(Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub(crate) enum ProposalResult {
    Applied { backup: String },
    Rejected,
    Refused { problem: Problem },
}

// Acquire before scheduling blocking work, including reads after a reopen.
static OWNER: tauri::async_runtime::Mutex<()> = tauri::async_runtime::Mutex::const_new(());

fn store(app: &tauri::AppHandle) -> Proposals {
    Proposals::beside(&app.state::<crate::preferences::SettingsFile>().path)
}

/// Waiting proposals and their engine previews, with library names only.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn waiting_proposals(
    app: tauri::AppHandle,
) -> Result<Vec<WaitingProposal>, crate::errors::NativeFailure> {
    crate::cli_install::with_owner(&OWNER, move || waiting(&store(&app))).await
}

fn waiting(store: &Proposals) -> Result<Vec<WaitingProposal>, crate::errors::NativeFailure> {
    Ok(store
        .waiting()
        .map_err(|_| crate::errors::NativeFailure)?
        .into_iter()
        .map(|preview| WaitingProposal {
            id: preview.proposal.id,
            source_app: preview.proposal.source.client,
            library: preview.library,
            summary: preview.summary,
            created_ms: preview.proposal.created_ms,
            lines: preview.lines,
            before: preview.before,
            after: preview.after,
            refusal: preview.refusal.map(WriteProblem::problem),
        })
        .collect())
}

/// Apply only after the person presses Apply on the displayed proposal.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn apply_proposal(
    app: tauri::AppHandle,
    id: String,
) -> Result<ProposalResult, crate::errors::NativeFailure> {
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
            problem: error.problem(),
        },
    }
}

/// Reject a waiting proposal; no library changes.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn reject_proposal(
    app: tauri::AppHandle,
    id: String,
) -> Result<ProposalResult, crate::errors::NativeFailure> {
    crate::cli_install::with_owner(&OWNER, move || Ok(reject(&store(&app), &id))).await
}

/// Reject all pending proposals with the same claimed app as this displayed id.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn reject_app_proposals(
    app: tauri::AppHandle,
    id: String,
) -> Result<ProposalResult, crate::errors::NativeFailure> {
    crate::cli_install::with_owner(&OWNER, move || {
        let store = store(&app);
        Ok(
            match store.get(&id).and_then(|proposal| store.reject_from(&proposal.source)) {
                Ok(_) => ProposalResult::Rejected,
                Err(error) => ProposalResult::Refused {
                    problem: error.problem(),
                },
            },
        )
    })
    .await
}

fn reject(store: &Proposals, id: &str) -> ProposalResult {
    match store.reject(id) {
        Ok(_) => ProposalResult::Rejected,
        Err(error) => ProposalResult::Refused {
            problem: error.problem(),
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
            std::fs::read_to_string(
                std::fs::read_dir(dir.path().join("library-backups"))
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path()
                    .join("signals.bib.ibis-backup-1")
            )
            .unwrap(),
            before
        );
        assert!(waiting(&store).unwrap().is_empty());
        assert_eq!(
            reject(&store, &proposal.id),
            ProposalResult::Refused {
                problem: WriteProblem::Decided.problem()
            }
        );
    }
}
