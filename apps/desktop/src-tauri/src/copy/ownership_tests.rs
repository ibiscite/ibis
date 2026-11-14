use super::*;
use crate::library::engine_library::FileLibrary;
use crate::picker::PickerGeneration;
use crate::project::ProjectBibliography;
use crate::session_references::SessionReferences;
use ibis_core::contract::DraftItem;
use serde_json::json;

#[test]
fn delayed_citations_write_only_for_their_original_picker_and_clipboard() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("invented.bib");
    std::fs::write(
        &path,
        "@book{taylor,title={Synthetic signals},author={Taylor, Alex},year={2026}}",
    )?;
    let file = FileLibrary::open(&path).map_err(|_| "open invented library")?;
    let query = serde_json::from_value(json!({"text":"signals","interpret_locators":false}))?;
    let rows = serde_json::to_value(file.search(&query, 10, None, &ibis_engine::Cancellation::default())?)?;
    let selection = json!({
        "id": rows["results"][0]["id"], "cite": rows["results"][0]["cite"], "locator": null
    });

    // Hiding alone and each clipboard counter independently retire a copy.
    // Hide/reopen and an equal-text replacement cannot revive it either.
    for (presentations, own_write, foreign_write, newer) in [
        (0, false, false, "Invented original clipboard"),
        (1, false, false, "Invented original clipboard"),
        (0, true, false, "Newer invented copy"),
        (0, false, true, "Newer invented copy"),
        (1, false, true, "Newer invented copy"),
        (2, false, false, "Invented original clipboard"),
        (0, false, true, "[@taylor]"),
    ] {
        let visits = PickerGeneration::default();
        let project = ProjectBibliography::default();
        let session = SessionReferences::default();
        let selection: DraftItem = serde_json::from_value(selection.clone())?;
        let preview = crate::test_citations::preview(&file.engine, file.sample, vec![selection], "pandoc")
            .map_err(|error| error.to_string())?;
        project
            .bind_preview(&file.engine, &preview, project.version())
            .map_err(|error| error.to_string())?;
        let mut mark = ClipboardMark {
            ours: 1,
            system: Some(10),
        };
        let intent = CopyIntent {
            presentation: visits.current(),
            clipboard: mark,
        };

        // Recitation stays queued until after the person hides or copies again.
        for _ in 0..presentations {
            visits.advance();
        }
        if own_write {
            mark.ours += 1;
        }
        if foreign_write {
            mark.system = Some(11);
        }
        let mut clipboard = newer.to_owned();
        let recited = file
            .engine
            .citations()
            .commit(
                ibis_engine::citations::SessionId::PICKER,
                &preview.ticket,
                &preview.content_hash,
            )
            .map_err(|error| error.to_string())?;
        let copied = commit_copy(
            &file,
            &recited,
            ProjectCopy {
                project: &project,
                session: &session,
            },
            |content| {
                intent.write(visits.current(), mark, || {
                    clipboard = content.text.to_owned();
                    Ok(Some(7))
                })
            },
        );

        if presentations == 0 && !own_write && !foreign_write {
            assert_eq!(copied.map_err(|error| error.to_string())?.delivery, Some(7));
            assert_eq!(clipboard, "[@taylor]");
            assert_eq!(session.items().len(), 1);
        } else {
            assert_eq!(copied.err(), Some(CommandError::Cancelled));
            assert_eq!(clipboard, newer);
            assert!(session.items().is_empty());
        }
    }
    Ok(())
}
