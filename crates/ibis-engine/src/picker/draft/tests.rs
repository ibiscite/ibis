use super::super::tests::{act, session, two_books};
use super::*;
use serde_json::json;

fn drafts_in(directory: &tempfile::TempDir) -> Drafts {
    Drafts::beside(&directory.path().join("settings.json"))
}

#[test]
fn a_citation_saved_by_one_engine_comes_back_in_another() -> io::Result<()> {
    let (_library, engine) = two_books();
    let mut picker = session(&engine, "apa");
    act(&mut picker, &engine, json!({"intent":"type","query":"vale 12"}));
    act(&mut picker, &engine, json!({"intent":"add"}));
    act(&mut picker, &engine, json!({"intent":"type","query":"fen"}));
    let state = act(&mut picker, &engine, json!({"intent":"add"}));
    let fen = state.citation[1].row.id.clone();
    act(&mut picker, &engine, json!({"intent":"reorder","item":fen,"step":-1}));
    let draft = picker.draft();
    assert_eq!(draft.items.len(), 2);
    assert!(draft.keep_order);

    let directory = tempfile::tempdir()?;
    drafts_in(&directory).save(&draft)?;
    let saved = fs::read_to_string(directory.path().join("picker-draft.json"))?;
    assert!(!saved.contains("vale 12"), "the typed search is never saved");
    let loaded = drafts_in(&directory).load();
    assert_eq!(loaded.as_ref(), Some(&draft));

    let mut restarted = session(&engine, "apa");
    let state = restarted.restore(&engine, loaded.unwrap_or_default());
    let cited: Vec<_> = state.citation.iter().map(|entry| entry.row.id.clone()).collect();
    assert_eq!(cited, draft.items.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>());
    assert!(state.citation.iter().all(|entry| entry.added));
    assert!(state.keep_order);
    assert!(
        state
            .citation
            .iter()
            .any(|entry| entry.note.as_deref() == Some("p. 12"))
    );
    assert!(!state.can_undo);
    Ok(())
}

#[test]
fn a_reference_the_library_no_longer_has_is_left_out() {
    let (_library, engine) = two_books();
    let mut picker = session(&engine, "apa");
    act(&mut picker, &engine, json!({"intent":"type","query":"fen"}));
    act(&mut picker, &engine, json!({"intent":"add"}));
    let mut draft = picker.draft();
    let mut gone = draft.items[0].0.clone();
    gone.key = "not-in-this-library".into();
    draft.items.insert(0, (gone, PickerDetails::default()));
    draft.held = Some("not-a-format".into());

    let mut restarted = session(&engine, "apa");
    let state = restarted.restore(&engine, draft);
    assert_eq!(state.citation.len(), 1);
    assert_eq!(restarted.draft().held, None, "a format Ibis does not offer is dropped");
}

#[test]
fn an_empty_or_stale_draft_leaves_nothing_behind() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let drafts = drafts_in(&directory);
    let path = directory.path().join("picker-draft.json");
    drafts.save(&PickerDraft::default())?;
    assert!(!path.exists());

    let stale = r#"{"version":1,"at_ms":1,"draft":{"items":[],"keep_order":false,"held":null}}"#;
    fs::write(&path, stale)?;
    assert_eq!(drafts.load(), None, "a draft older than ten minutes is ignored");
    drafts.save(&PickerDraft::default())?;
    assert!(!path.exists(), "a delivered or emptied citation removes the file");

    fs::write(&path, "not json")?;
    assert_eq!(drafts.load(), None);
    Ok(())
}

#[test]
fn turning_recent_citations_off_removes_the_draft() -> io::Result<()> {
    let (_library, engine) = two_books();
    let mut picker = session(&engine, "apa");
    act(&mut picker, &engine, json!({"intent":"type","query":"fen"}));
    act(&mut picker, &engine, json!({"intent":"add"}));
    let directory = tempfile::tempdir()?;
    let settings = directory.path().join("settings.json");
    drafts_in(&directory).save(&picker.draft())?;
    crate::recents::clear_disabled(&settings)?;
    assert!(
        drafts_in(&directory).load().is_some(),
        "Recent citations is on by default"
    );
    fs::write(&settings, r#"{"schema_version":2,"recent_citations":false}"#)?;
    crate::recents::clear_disabled(&settings)?;
    assert!(!directory.path().join("picker-draft.json").exists());
    Ok(())
}
