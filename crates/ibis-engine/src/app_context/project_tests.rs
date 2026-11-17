use super::*;

fn written(name: &str, text: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let path = dir.path().join(name);
    std::fs::write(&path, text).unwrap_or_else(|error| panic!("{error}"));
    (dir, path)
}

fn read(ticket: &Ticket) -> ProjectRead {
    ProjectRead::read(&ticket.path, ticket.previous.as_ref())
}

fn binding(engine: Option<crate::Engine>) -> (crate::Engine, ibis_core::contract::Preview) {
    let engine = engine.unwrap_or_else(|| {
        let case: ibis_core::contract::GoldenCase =
            serde_json::from_str(include_str!("../../../../fixtures/cases/02-pandoc.json")).unwrap();
        crate::Engine::from_snapshot(std::sync::Arc::new(case.library))
    });
    let snapshot = engine.snapshot().unwrap();
    let row = &snapshot.objects()[0];
    let draft=serde_json::from_value(serde_json::json!({"items":[{"id":row.object.id(),"cite":row.fingerprints.cite}],"output":{"kind":"preset","name":"pandoc"}})).unwrap();
    let preview = engine
        .citations()
        .preview(
            crate::citations::SessionId::PICKER,
            draft,
            crate::Cancellation::default(),
        )
        .unwrap()
        .run()
        .unwrap();
    (engine, preview)
}

const A: &str = "@book{a2026,title={A}}\n";

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
        Ok::<_, super::super::Error>(())
    })
    .unwrap();
    assert_eq!(written, "citation A");
    slot.choose(None);
    assert!(
        slot.commit(&preview.ticket, |_| {
            written.push_str("wrong");
            Ok::<_, super::super::Error>(())
        })
        .is_err()
    );
    slot.choose(None);
    assert!(slot.bind_preview(&engine, &preview, a).is_err());
    assert!(
        slot.commit(&preview.ticket, |_| {
            written.push_str("wrong");
            Ok::<_, super::super::Error>(())
        })
        .is_err()
    );
    assert_eq!(written, "citation A");
    let (engine, fresh) = binding(Some(engine));
    slot.bind_preview(&engine, &fresh, slot.version()).unwrap();
    slot.commit(&fresh.ticket, |_| {
        written.push_str(" fresh A");
        Ok::<_, super::super::Error>(())
    })
    .unwrap();
    assert_eq!(written, "citation A fresh A");
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
    assert!(
        slot.next_read(a.generation).is_none(),
        "and A's reader takes no other work"
    );
    assert!(slot.next_read(b.generation).is_none());
    assert!(slot.lock().readers.is_empty());

    // Quick switching keeps at most one replaced read beside the current
    // one; later choices wait, and only the latest of them is read.
    let c = slot
        .enqueue_read(slot.choose(Some("/invented/c.bib".into())).unwrap())
        .unwrap();
    let d = slot
        .enqueue_read(slot.choose(Some("/invented/d.bib".into())).unwrap())
        .unwrap();
    for name in ["e.bib", "f.bib"] {
        let queued = slot.choose(Some(std::path::Path::new("/invented").join(name))).unwrap();
        assert!(slot.enqueue_read(queued).is_none());
    }
    let f = slot
        .next_read(c.generation)
        .unwrap_or_else(|| panic!("the latest choice"));
    assert_eq!(f.path.file_name().unwrap(), "f.bib");
    assert!(slot.next_read(d.generation).is_none());
    assert!(slot.next_read(f.generation).is_none());
    assert!(slot.lock().readers.is_empty());
    let g = slot
        .enqueue_read(slot.choose(Some("/invented/g.bib".into())).unwrap())
        .unwrap();
    slot.enqueue_read(slot.choose(Some("/invented/h.bib".into())).unwrap())
        .unwrap();
    let abandoned = slot.choose(Some("/invented/i.bib".into())).unwrap();
    assert!(slot.enqueue_read(abandoned).is_none());
    slot.choose(None);
    assert!(
        slot.next_read(g.generation).is_none(),
        "leaving a project drops its queued read"
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
    let output = Some(super::super::output::stored(Some("pandoc")));
    assert!(
        slot.capture(None, output).is_err(),
        "an unowned request cannot prepare a citation"
    );
    assert!(slot.capture(Some(slot.version()), output).is_err());
    assert!(
        slot.capture(Some(slot.version()), Some(super::super::output::stored(Some("apa"))))
            .is_ok(),
        "formatted text does not check bibliography keys"
    );
    assert!(
        slot.commit::<(), super::super::Error>("not-prepared", |_| panic!("must not write before checking"))
            .is_err()
    );
    slot.finish(ticket.generation, read(&ticket));
    assert!(slot.capture(Some(slot.version()), output).is_ok());
    let (engine, preview) = binding(None);
    slot.bind_preview(&engine, &preview, slot.version()).unwrap();
    slot.commit(&preview.ticket, |current| {
        assert_eq!(current.unwrap().name, "paper.bib");
        Ok::<_, super::super::Error>(())
    })
    .unwrap();
}
