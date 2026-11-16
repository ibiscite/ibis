use super::*;

fn owner() -> (tempfile::TempDir, Owner, Capture) {
    let directory = tempfile::tempdir().unwrap();
    let settings = Arc::new(settings::Owner::open(directory.path().join("settings.json")));
    let owner = Owner::new(settings, project::ProjectBibliography::default());
    let capture = Capture {
        app: Some(AppId {
            system: System::Macos,
            id: "org.example.Writer".into(),
        }),
        generation: 1,
        editor: None,
    };
    (directory, owner, capture)
}

fn request(owner: &Owner, capture: &Capture, request: ContextRequest) -> ContextView {
    owner.request(capture, request, || true).unwrap()
}

#[test]
fn one_request_keeps_writing_format_and_project_together_and_leaving_restores_the_app() {
    let (directory, owner, capture) = owner();
    owner
        .settings
        .change::<_, Error>(|settings| {
            settings.remember_app_output(capture.app.as_ref().unwrap(), "typst".into());
            Ok(())
        })
        .unwrap();
    let initial = request(&owner, &capture, ContextRequest::Writings {});
    assert_eq!(initial.format.id.as_deref(), Some("typst"));
    let created = request(
        &owner,
        &capture,
        ContextRequest::EditWriting {
            context: initial.writings.context,
            action: WritingEdit::Create {
                name: "Paper".into(),
                output: output::stored(Some("pandoc")),
            },
        },
    );
    assert_eq!(created.format.id.as_deref(), Some("pandoc"));
    assert_eq!(created.format.from, crate::format::FormatFrom::Writing);
    assert_eq!(created.writings.active, created.writing);
    assert_eq!(created.writings.selected.as_deref(), Some("Paper"));
    assert_eq!(created.writing_context.as_ref(), Some(&created.writings.context));
    let bibliography = directory.path().join("refs.bib");
    std::fs::write(&bibliography, "@book{invented,title={Invented}}\n").unwrap();
    let chooser = owner.bibliography_owner(&capture, || true).unwrap();
    owner
        .set_bibliography(&capture, &chooser, Some(bibliography), || true)
        .unwrap();
    let checking = request(&owner, &capture, ContextRequest::ProjectStatus {});
    let status = checking.project.unwrap();
    assert_eq!(status.name, "refs.bib");
    assert_eq!((status.generation, status.revision), checking.context_version);
    assert_eq!(checking.writing.unwrap().bibliography.as_deref(), Some("refs.bib"));
    let stopped = request(&owner, &capture, ContextRequest::StopProject {});
    assert!(stopped.project.is_none());
    assert_eq!(stopped.format.id.as_deref(), Some("pandoc"));
    let left = request(
        &owner,
        &capture,
        ContextRequest::EditWriting {
            context: stopped.writings.context,
            action: WritingEdit::Select { id: None },
        },
    );
    assert!(left.writing.is_none());
    assert_eq!(left.format.id.as_deref(), Some("typst"));
}

#[test]
fn stale_openings_and_retired_destinations_cannot_save_a_writing() {
    let (_directory, owner, mut capture) = owner();
    let view = request(&owner, &capture, ContextRequest::Writings {});
    capture.generation += 1;
    let edit = ContextRequest::EditWriting {
        context: view.writings.context,
        action: WritingEdit::Create {
            name: "Stale".into(),
            output: output::stored(Some("pandoc")),
        },
    };
    assert!(matches!(
        owner.request(&capture, edit.clone(), || true),
        Err(Error::Destination)
    ));
    assert!(matches!(
        owner.request(&capture, edit, || false),
        Err(Error::Destination)
    ));
    assert!(owner.settings.read().cites().unwrap().device.writings.is_empty());
    let fresh = request(&owner, &capture, ContextRequest::Writings {});
    owner
        .settings
        .change::<_, Error>(|settings| {
            settings.remember_app_output(capture.app.as_ref().unwrap(), "typst".into());
            Ok(())
        })
        .unwrap();
    let stale = ContextRequest::EditWriting {
        context: fresh.writings.context,
        action: WritingEdit::Create {
            name: "Stale format".into(),
            output: output::stored(Some("pandoc")),
        },
    };
    assert!(matches!(
        owner.request(&capture, stale, || true),
        Err(Error::Destination)
    ));
    assert!(owner.settings.read().cites().unwrap().device.writings.is_empty());
}

#[test]
fn editor_context_is_self_contained_and_cannot_edit_the_apps_writings() {
    let (directory, owner, mut capture) = owner();
    owner
        .project
        .activate("previous-app".into(), Some(directory.path().join("previous.bib")), None);
    std::fs::write(owner.settings.path(), "unreadable settings").unwrap();
    capture.editor = Some(EditorContext {
        output: output::stored(Some("autocite")).spec().clone(),
        writing: None,
    });
    let view = request(&owner, &capture, ContextRequest::Destination {});
    assert_eq!(view.format.id.as_deref(), Some("autocite"));
    assert!(view.project.is_none());
    assert!(view.writing.is_none());
    assert!(view.writing_context.is_none());
    assert!(!view.paste_enabled);
    assert!(matches!(
        owner.request(&capture, ContextRequest::StopProject {}, || true),
        Err(Error::Editor)
    ));
}

#[test]
fn unreadable_settings_retire_the_previous_context_without_using_the_backup() {
    let (_directory, owner, capture) = owner();
    request(&owner, &capture, ContextRequest::Destination {});
    let before = owner.project.version();
    std::fs::write(owner.settings.path(), "unreadable settings").unwrap();
    assert!(
        owner
            .request(&capture, ContextRequest::Destination {}, || true)
            .is_err()
    );
    assert_ne!(owner.project.version(), before);
    assert!(
        owner
            .project
            .capture(Some(owner.project.version()), Some(output::stored(Some("apa"))))
            .is_err()
    );
}

#[test]
fn remembered_formats_are_read_by_the_context_owner_after_reopening() {
    let (directory, owner, capture) = owner();
    for output in ["pandoc", "typst", "a-later-output"] {
        owner
            .settings
            .change::<_, Error>(|settings| {
                settings.remember_app_output(capture.app.as_ref().unwrap(), output.into());
                Ok(())
            })
            .unwrap();
        let reopened = Owner::new(
            Arc::new(settings::Owner::open(directory.path().join("settings.json"))),
            project::ProjectBibliography::default(),
        );
        let view = request(&reopened, &capture, ContextRequest::Destination {});
        assert_eq!(
            view.format.id.as_deref(),
            Some(if output == "a-later-output" { "apa" } else { output })
        );
    }
}

#[test]
fn suggested_and_remembered_outputs_commit_the_exact_preview() {
    use crate::{Cancellation, Engine, citations::SessionId};
    use ibis_core::contract::{Draft, DraftItem, OutputSpec};
    let (directory, owner, mut capture) = owner();
    let path = directory.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@article{demo,title={Synthetic research},author={Researcher, Synthetic},year={2026}}\n",
    )
    .unwrap();
    let engine = Engine::new().unwrap();
    engine.open_file_observed(&path, &Cancellation::default()).unwrap();
    let snapshot = engine.snapshot().unwrap();
    let row = &snapshot.objects()[0];
    let selection: DraftItem =
        serde_json::from_value(serde_json::json!({"id":row.object.id(),"cite":row.fingerprints.cite})).unwrap();
    let mut cases = vec![
        ("com.microsoft.Word", None, "(Researcher, 2026)".to_owned()),
        ("TeXShop", None, "\\autocite{demo}".to_owned()),
    ];
    cases.extend(crate::outputs::outputs().iter().filter_map(|output| {
        let example = output
            .example
            .as_ref()
            .filter(|_| matches!(output.output, OutputSpec::Key { .. }) && !output.zotero_items)?;
        Some((
            "org.example.Writer",
            Some(output.id.as_str()),
            example.replace("key", "demo"),
        ))
    }));
    for (id, output, expected) in cases {
        capture.app.as_mut().unwrap().id = id.into();
        if let Some(output) = output {
            owner
                .settings
                .change::<_, Error>(|settings| {
                    settings.remember_app_output(capture.app.as_ref().unwrap(), output.into());
                    Ok(())
                })
                .unwrap();
        }
        let view = request(&owner, &capture, ContextRequest::Destination {});
        let style = matches!(view.format.output, OutputSpec::Csl { .. });
        let draft = Draft {
            items: vec![selection.clone()],
            output: view.format.output,
            form: Default::default(),
            keep_order: false,
            input_language: None,
            output_locale: None,
            sample: false,
        };
        let preview = engine
            .citations()
            .preview(SessionId::PICKER, draft, Cancellation::default())
            .unwrap()
            .run()
            .unwrap();
        assert_eq!(preview.text, expected);
        let committed = engine
            .citations()
            .commit(SessionId::PICKER, &preview.ticket, &preview.content_hash)
            .unwrap();
        let guard = committed.check(&engine).unwrap();
        let content = guard.content();
        assert_eq!(content.text, preview.text);
        assert_eq!(content.html, style.then_some(preview.text.as_str()));
    }
}
