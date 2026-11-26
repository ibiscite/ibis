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
        site: None,
        generation: 1,
        editor: None,
    };
    (directory, owner, capture)
}

fn request(owner: &Owner, capture: &Capture, request: ContextRequest) -> ContextView {
    owner.request(capture, request, || true).unwrap()
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
fn a_saved_site_format_change_retires_prepared_citations_in_the_same_opening() {
    let directory = tempfile::tempdir().unwrap();
    let settings = Arc::new(settings::Owner::open(directory.path().join("settings.json")));
    let bibliography = project::ProjectBibliography::default();
    let owner = Owner::new(settings.clone(), bibliography.clone());
    let capture = Capture {
        app: Some(AppId {
            system: System::Macos,
            id: "com.google.Chrome".into(),
        }),
        site: ibis_core::destination::site("https://overleaf.com/project/invented"),
        generation: 1,
        editor: None,
    };
    let before = request(&owner, &capture, ContextRequest::Destination {});
    let old_output = output::stored(before.format.id.as_deref());
    assert!(
        bibliography
            .capture(Some(before.context_version), Some(old_output))
            .is_ok()
    );
    settings
        .change::<_, Error>(|settings| {
            settings.remember_app_output(capture.site.as_ref().unwrap(), "typst".into());
            Ok(())
        })
        .unwrap();
    let after = request(&owner, &capture, ContextRequest::Destination {});
    assert_eq!(after.format.id.as_deref(), Some("typst"));
    assert_eq!(after.remembered_for, capture.site);
    assert_ne!(after.context_version.0, before.context_version.0);
    assert!(matches!(
        bibliography.capture(Some(before.context_version), Some(old_output)),
        Err(Error::Cancelled)
    ));
    assert!(
        bibliography
            .capture(Some(after.context_version), Some(output::stored(Some("typst"))))
            .is_ok()
    );
}
