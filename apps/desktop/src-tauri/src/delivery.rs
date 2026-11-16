//! One picker intent, from a checked citation to its native receipt.

use ibis_core::contract::{CitedWarning, Problem};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::copy::{Copied, CopyIntent, clipboard_mark};
use crate::library::CommandError;
use crate::paste::{CopiedReason, Delivered};
use ibis_engine::citations::SessionId;
use ibis_engine::library::Owner;

#[derive(Serialize, schemars::JsonSchema)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub(crate) enum DeliveryOutcome {
    Pasted {
        copied: Copied,
        app: Option<String>,
    },
    CopiedOnly {
        copied: Copied,
        reason: CopiedReason,
    },
    Retained {
        copied: Copied,
        reason: RetainedReason,
    },
    Sent {
        copied: Copied,
        retained: Option<RetainedReason>,
    },
    Refused {
        problem: Problem,
    },
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RetainedReason {
    Warnings,
    Pinned,
    HideFailed,
    PresentationChanged,
}

/// Search provenance retained with each selected reference.
#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ClosestMatch {
    item: usize,
    title: String,
}

/// The preview ticket fixes the citation's context. The presentation fixes the
/// picker it may copy for and hide; nothing received later can adopt this work.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn deliver_citation(
    ticket: String,
    content_hash: String,
    replaces: Vec<String>,
    presentation: u64,
    closest_matches: Vec<ClosestMatch>,
    app: AppHandle,
) -> DeliveryOutcome {
    let prepared = prepare(
        ticket,
        content_hash,
        replaces,
        closest_matches,
        app.clone(),
        presentation,
    )
    .await;
    complete(prepared, async |copied| {
        crate::notice::deliver(
            app.clone(),
            copied.text.clone(),
            copied.delivery,
            copied.summary.clone(),
        )
        .await
    })
    .await
}

/// Copy the shown citation. Citing it again can wait for the formatter, so
/// that runs on a worker with no lock held; the short commit that writes the
/// clipboard stays on the main thread, as the native clipboard needs.
async fn prepare(
    ticket: String,
    content_hash: String,
    replaces: Vec<String>,
    closest_matches: Vec<ClosestMatch>,
    app: AppHandle,
    presentation: u64,
) -> Result<Ready, CommandError> {
    let library = tauri::Manager::state::<Owner>(&app);
    let intent = CopyIntent {
        presentation,
        clipboard: clipboard_mark(),
    };
    // What the copy is for is fixed when it is asked for: a Forget or a new
    // target while it waits must not adopt it.
    let editor = tauri::Manager::state::<crate::editor_endpoint::Pending>(&app);
    let captured = editor.app(crate::paste::last_app(&app));
    let epoch = tauri::Manager::state::<crate::session_references::SessionReferences>(&app)
        .lock()
        .epoch();
    let file = {
        let library = library.view();
        if library.opening {
            return Err(CommandError::LibraryLoading);
        }
        library.current.ok_or(CommandError::SnapshotChanged)?
    };
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
        let result =
            crate::copy::commit_shown(&main, &recited, replaces, captured, epoch, intent).map(|(mut copied, sent)| {
                copied
                    .warnings
                    .extend(closest_matches.into_iter().map(|closest| CitedWarning {
                        item: Some(closest.item),
                        message: ibis_core::contract::Message {
                            id: "picker-closest-match-warning".into(),
                            args: [("title".into(), closest.title)].into(),
                            context: None,
                        },
                    }));
                // Commit and dismissal share one main-thread task. No delayed
                // frontend step can let a newer pin or presentation take this copy.
                finish(
                    copied,
                    sent,
                    main.state::<crate::picker::Pinned>()
                        .0
                        .load(std::sync::atomic::Ordering::SeqCst),
                    || crate::picker::hide_picker(main.clone(), Some(presentation)),
                )
            });
        let _ = send.send(result);
    })
    .map_err(|_| CommandError::ClipboardUnavailable)?;
    tauri::async_runtime::spawn_blocking(move || committed.recv())
        .await
        .map_err(|_| CommandError::ClipboardUnavailable)?
        .map_err(|_| CommandError::ClipboardUnavailable)?
}

struct Ready {
    copied: Copied,
    sent: bool,
    retained: Option<RetainedReason>,
}

fn finish(copied: Copied, sent: bool, pinned: bool, hide: impl FnOnce() -> Result<bool, CommandError>) -> Ready {
    let retained = if !copied.warnings.is_empty() {
        Some(RetainedReason::Warnings)
    } else if pinned {
        Some(RetainedReason::Pinned)
    } else {
        match hide() {
            Ok(true) => None,
            Ok(false) => Some(RetainedReason::PresentationChanged),
            Err(_) => Some(RetainedReason::HideFailed),
        }
    };
    Ready { copied, sent, retained }
}

/// Only a committed, dismissed citation can reach native paste and its receipt.
async fn complete(
    prepared: Result<Ready, CommandError>,
    paste: impl AsyncFnOnce(&Copied) -> Delivered,
) -> DeliveryOutcome {
    let Ready { copied, sent, retained } = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let mut problem = error.problem();
            if problem.code == "cancelled" {
                return DeliveryOutcome::Cancelled;
            }
            // Keep the picker's existing sentences while carrying the typed problem.
            problem.info.message.id = match problem.code.as_str() {
                "snapshot_changed" | "changed-item" | "missing-item" => "picker-snapshot-changed",
                "ambiguous-key" => "picker-shared-key",
                _ => "picker-copy-error",
            }
            .into();
            return DeliveryOutcome::Refused { problem };
        }
    };
    if let Some(reason) = retained {
        return if sent {
            DeliveryOutcome::Sent {
                copied,
                retained: Some(reason),
            }
        } else {
            DeliveryOutcome::Retained { copied, reason }
        };
    }
    let delivered = paste(&copied).await;
    if sent {
        return DeliveryOutcome::Sent { copied, retained: None };
    }
    match delivered {
        Delivered::Pasted { app } => DeliveryOutcome::Pasted { copied, app },
        Delivered::Copied { reason } => DeliveryOutcome::CopiedOnly { copied, reason },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::CopyReason;
    use std::cell::Cell;

    fn copied() -> Copied {
        Copied {
            text: "[@synthetic]".into(),
            warnings: vec![],
            summary: None,
            delivery: Some(7),
            cited: vec![],
        }
    }

    #[test]
    fn delivery_command_preserves_each_native_outcome_and_runs_effects_once() {
        tauri::async_runtime::block_on(async {
            for delivered in [
                Delivered::Pasted {
                    app: Some("Example Writer".into()),
                },
                Delivered::copied(CopyReason::NoPermission),
            ] {
                let hides = Cell::new(0);
                let pastes = Cell::new(0);
                let ready = finish(copied(), false, false, || {
                    hides.set(hides.get() + 1);
                    Ok(true)
                });
                let result = complete(Ok(ready), async |_| {
                    pastes.set(pastes.get() + 1);
                    delivered.clone()
                })
                .await;
                match (result, delivered) {
                    (DeliveryOutcome::Pasted { copied, app }, Delivered::Pasted { app: expected }) => {
                        assert_eq!(copied.text, "[@synthetic]");
                        assert_eq!(app, expected);
                    }
                    (DeliveryOutcome::CopiedOnly { copied, reason }, Delivered::Copied { reason: expected }) => {
                        assert_eq!(copied.delivery, Some(7));
                        assert_eq!(reason, expected);
                    }
                    _ => panic!("delivery must preserve the native outcome"),
                }
                assert_eq!((hides.get(), pastes.get()), (1, 1));
            }
        });
    }

    #[test]
    fn delivery_command_retains_warnings_pins_and_failed_or_obsolete_hides() {
        tauri::async_runtime::block_on(async {
            for reason in [
                RetainedReason::Warnings,
                RetainedReason::Pinned,
                RetainedReason::HideFailed,
                RetainedReason::PresentationChanged,
            ] {
                let mut copied = copied();
                if reason == RetainedReason::Warnings {
                    copied.warnings.push(serde_json::from_value(serde_json::json!({
                        "item": 0, "message": { "id": "picker-closest-match-warning", "args": { "title": "Invented work" } }
                    })).unwrap_or_else(|_| panic!("invented warning")));
                }
                let ready = finish(copied, false, reason == RetainedReason::Pinned, || match reason {
                    RetainedReason::HideFailed => Err(CommandError::ClipboardUnavailable),
                    RetainedReason::PresentationChanged => Ok(false),
                    _ => panic!("warnings and pins never hide"),
                });
                let result = complete(Ok(ready), async |_| panic!("retained copies never paste")).await;
                assert!(matches!(result, DeliveryOutcome::Retained { reason: actual, .. } if actual == reason));
            }
        });
    }

    #[test]
    fn delivery_command_refusals_and_cancellation_have_no_native_effects() {
        tauri::async_runtime::block_on(async {
            for error in [
                CommandError::Cancelled,
                CommandError::from(ibis_engine::citations::SessionError::Cancelled),
                CommandError::SnapshotChanged,
                CommandError::ClipboardUnavailable,
            ] {
                let cancelled = error.problem().code == "cancelled";
                let result = complete(Err(error), async |_| panic!("nothing to paste")).await;
                match result {
                    DeliveryOutcome::Cancelled => assert!(cancelled),
                    DeliveryOutcome::Refused { problem } => {
                        assert!(!cancelled);
                        let sentence = ibis_engine::catalogue::problem(&problem, ibis_engine::catalogue::Audience::App);
                        assert!(
                            sentence == "Nothing was copied. Try again, or use Reload."
                                || sentence
                                    == "The library changed. Search again and reselect the references before copying."
                        );
                    }
                    _ => panic!("failed preparation cannot deliver"),
                }
            }
        });
    }

    #[test]
    fn delivery_command_reports_a_committed_editor_answer() {
        tauri::async_runtime::block_on(async {
            let ready = finish(copied(), true, true, || panic!("pinned answer stays open"));
            assert!(matches!(
                complete(Ok(ready), async |_| panic!("pinned answer never pastes")).await,
                DeliveryOutcome::Sent { .. }
            ));
        });
    }
}
