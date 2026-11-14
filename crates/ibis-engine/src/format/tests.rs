use super::*;

#[test]
fn forgetting_a_captured_app_restores_your_format_despite_identifier_case() {
    let word = AppId {
        system: ibis_core::destination::System::Macos,
        id: "com.microsoft.Word".into(),
    };
    let mut settings = Settings {
        portable: crate::settings::Portable {
            output: Some("modern-language-association".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    settings.remember_app_output(&word, "apa".into());
    assert_eq!(choose(&settings, Some(&word)).from, FormatFrom::App);
    settings.forget_app_output(&word);
    let restored = choose(&settings, Some(&word));
    assert_eq!(restored.from, FormatFrom::Yours);
    assert_eq!(restored.id.as_deref(), Some("modern-language-association"));
}
use crate::settings::writings::Writing;
use ibis_core::contract::KeySyntax;
use ibis_core::destination::System::{Macos, Windows};

fn app(system: ibis_core::destination::System, id: &str) -> AppId {
    AppId { system, id: id.into() }
}

/// The output id and the level that chose it, as the header reads them.
fn said(settings: &Settings, target: Option<&AppId>) -> (String, FormatFrom) {
    let chosen = choose(settings, target);
    (chosen.id.unwrap_or_default(), chosen.from)
}

fn cite(chosen: &FormatChoice) -> Result<String, Box<dyn std::error::Error>> {
    let mut case: ibis_core::contract::GoldenCase =
        serde_json::from_str(include_str!("../../../../fixtures/cases/02-pandoc.json"))?;
    case.request.output = chosen.output.clone();
    let engine = crate::Engine::from_snapshot(std::sync::Arc::new(case.library));
    let result = engine
        .cite(&case.request, &crate::Cancellation::default())
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        result.outcomes,
        [ibis_core::contract::ItemOutcome::Rendered { item: 0 }]
    );
    assert!(result.copy_content(1).is_some());
    Ok(result.text)
}

#[test]
fn a_saved_bundled_style_remains_your_document_default_after_reopen() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.json");
    crate::settings::Owner::open(path.clone()).change(|settings| {
        settings.portable.output = Some("abi-technik".into());
        settings.portable.key_output = Some("typst".into());
        Ok::<_, crate::settings::SettingsError>(())
    })?;
    let loaded = crate::settings::load(&path);
    assert!(loaded.problem.is_none());
    for target in [
        None,
        Some(app(Macos, "com.microsoft.word")),
        Some(app(Macos, "org.example.invented")),
    ] {
        let chosen = choose(&loaded.settings, target.as_ref());
        assert_eq!(
            chosen,
            FormatChoice {
                output: OutputSpec::Csl {
                    style: "abi-technik".into()
                },
                id: Some("abi-technik".into()),
                from: FormatFrom::Yours,
                kind: FormatKind::Documents,
                writing: None,
            }
        );
        assert_eq!(
            cite(&chosen)?,
            "Avery Taylor, Synthetic Signals (Imaginary Press, 2026)."
        );
    }
    let markdown = app(Macos, "md.obsidian");
    let chosen = choose(&loaded.settings, Some(&markdown));
    assert_eq!(
        chosen,
        FormatChoice {
            output: OutputSpec::Key {
                syntax: KeySyntax::Typst,
                command: None
            },
            id: Some("typst".into()),
            from: FormatFrom::Yours,
            kind: FormatKind::Keys,
            writing: None,
        }
    );
    assert_eq!(cite(&chosen)?, "@Example2026");
    let mut settings = loaded.settings;
    // A bundled style in the key setting is still no key choice.
    settings.portable.key_output = Some("abi-technik".into());
    let chosen = choose(&settings, Some(&markdown));
    assert_eq!(
        chosen,
        FormatChoice {
            output: OutputSpec::Key {
                syntax: KeySyntax::Pandoc,
                command: None
            },
            id: Some("pandoc".into()),
            from: FormatFrom::Suggestion,
            kind: FormatKind::Keys,
            writing: None,
        }
    );
    assert_eq!(cite(&chosen)?, "[@Example2026]");
    Ok(())
}

#[test]
fn a_saved_bundled_app_override_wins_and_keeps_its_id_and_output() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.json");
    let markdown = app(Macos, "md.obsidian");
    crate::settings::Owner::open(path.clone()).change(|settings| {
        settings.portable.key_output = Some("typst".into());
        settings.remember_app_output(&markdown, "abi-technik".into());
        Ok::<_, crate::settings::SettingsError>(())
    })?;
    let loaded = crate::settings::load(&path);
    assert!(loaded.problem.is_none());
    let pandoc = OutputSpec::Key {
        syntax: KeySyntax::Pandoc,
        command: None,
    };
    let chosen = choose_with(&loaded.settings, Some(&markdown), Some(&pandoc));
    assert_eq!(
        chosen,
        FormatChoice {
            output: OutputSpec::Csl {
                style: "abi-technik".into()
            },
            id: Some("abi-technik".into()),
            from: FormatFrom::App,
            kind: FormatKind::Keys,
            writing: None,
        }
    );
    assert_eq!(
        cite(&chosen)?,
        "Avery Taylor, Synthetic Signals (Imaginary Press, 2026)."
    );
    Ok(())
}

#[test]
fn unknown_saved_ids_keep_the_suggestion_for_each_kind() -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = Settings::default();
    settings.portable.output = Some("invented-unavailable-style".into());
    settings.portable.key_output = Some("invented-unavailable-key-output".into());
    for (target, id, output, kind, text) in [
        (
            app(Macos, "com.microsoft.word"),
            "apa",
            OutputSpec::Csl { style: "apa".into() },
            FormatKind::Documents,
            "(Taylor, 2026)",
        ),
        (
            app(Macos, "md.obsidian"),
            "pandoc",
            OutputSpec::Key {
                syntax: KeySyntax::Pandoc,
                command: None,
            },
            FormatKind::Keys,
            "[@Example2026]",
        ),
    ] {
        settings.remember_app_output(&target, "invented-unavailable-app-output".into());
        let chosen = choose(&settings, Some(&target));
        assert_eq!(
            chosen,
            FormatChoice {
                output,
                id: Some(id.into()),
                from: FormatFrom::Suggestion,
                kind,
                writing: None
            }
        );
        assert_eq!(cite(&chosen)?, text);
    }
    Ok(())
}

#[test]
fn with_nothing_chosen_each_kind_of_app_gets_its_suggestion() {
    let settings = Settings::default();
    for (target, id, kind) in [
        (app(Macos, "com.microsoft.word"), "apa", FormatKind::Documents),
        (app(Macos, "md.obsidian"), "pandoc", FormatKind::Keys),
        (app(Windows, "texstudio.exe"), "autocite", FormatKind::Keys),
        (app(Macos, "com.microsoft.vscode"), "pandoc", FormatKind::Keys),
        (app(Macos, "com.apple.safari"), "apa", FormatKind::Documents),
        (app(Macos, "org.example.invented"), "apa", FormatKind::Documents),
    ] {
        let chosen = choose(&settings, Some(&target));
        assert_eq!(
            (chosen.id.as_deref(), chosen.from, chosen.kind),
            (Some(id), FormatFrom::Suggestion, kind),
            "{}",
            target.id
        );
    }
    assert_eq!(said(&settings, None), ("apa".into(), FormatFrom::Suggestion));
}

#[test]
fn your_format_applies_only_to_apps_of_its_own_kind() {
    let settings = Settings {
        portable: crate::settings::Portable {
            output: Some("chicago-author-date".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let word = app(Macos, "com.microsoft.word");
    let obsidian = app(Macos, "md.obsidian");
    assert_eq!(
        said(&settings, Some(&word)),
        ("chicago-author-date".into(), FormatFrom::Yours)
    );
    // A style chosen for documents never reaches a Markdown file.
    assert_eq!(
        said(&settings, Some(&obsidian)),
        ("pandoc".into(), FormatFrom::Suggestion)
    );
    let settings = Settings {
        portable: crate::settings::Portable {
            key_output: Some("typst".into()),
            ..settings.portable
        },
        device: settings.device,
        unknown: settings.unknown,
    };
    assert_eq!(said(&settings, Some(&obsidian)), ("typst".into(), FormatFrom::Yours));
    assert_eq!(
        said(&settings, Some(&word)),
        ("chicago-author-date".into(), FormatFrom::Yours)
    );
    // A key id left in the documents value, or a style in the key value, is no choice.
    let crossed = Settings {
        portable: crate::settings::Portable {
            output: Some("pandoc".into()),
            key_output: Some("apa".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(said(&crossed, Some(&word)), ("apa".into(), FormatFrom::Suggestion));
    assert_eq!(
        said(&crossed, Some(&obsidian)),
        ("pandoc".into(), FormatFrom::Suggestion)
    );
}

#[test]
fn an_apps_own_format_beats_yours_whatever_its_kind() {
    let obsidian = app(Macos, "md.obsidian");
    let mut settings = Settings {
        portable: crate::settings::Portable {
            key_output: Some("typst".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    settings.remember_app_output(&obsidian, "modern-language-association".into());
    assert_eq!(
        said(&settings, Some(&obsidian)),
        ("modern-language-association".into(), FormatFrom::App)
    );
    // An id this Ibis does not list reads as no choice.
    settings.remember_app_output(&obsidian, "invented-output".into());
    assert_eq!(said(&settings, Some(&obsidian)), ("typst".into(), FormatFrom::Yours));
}

#[test]
fn a_writing_chosen_for_the_app_beats_the_apps_format_and_says_its_name() -> Result<(), Box<dyn std::error::Error>> {
    let word = app(Macos, "com.microsoft.word");
    let mut settings = Settings::default();
    settings.remember_app_output(&word, "abi-technik".into());
    let thesis: Writing = serde_json::from_value(serde_json::json!({
        "name": "Thesis",
        "format": {"kind": "csl", "style": "chicago-notes-bibliography"},
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    settings.device.writings.push(thesis);
    settings
        .device
        .writing_apps
        .insert("macos:com.microsoft.word".into(), "Thesis".into());
    let chosen = choose(&settings, Some(&word));
    assert_eq!(
        chosen,
        FormatChoice {
            output: OutputSpec::Csl {
                style: "chicago-notes-bibliography".into()
            },
            id: Some("chicago-notes-bibliography".into()),
            from: FormatFrom::Writing,
            kind: FormatKind::Documents,
            writing: Some(WritingName {
                id: "Thesis".into(),
                name: "Thesis".into()
            }),
        }
    );
    assert_eq!(
        cite(&chosen)?,
        "Avery Taylor, Synthetic Signals (Imaginary Press, 2026)."
    );
    // Another app keeps its own answer.
    let pages = app(Macos, "com.apple.iwork.pages");
    assert_eq!(said(&settings, Some(&pages)), ("apa".into(), FormatFrom::Suggestion));
    Ok(())
}

#[test]
fn a_named_format_stands_in_for_yours_and_an_unknown_app_keeps_it_whatever_its_kind() {
    let settings = Settings::default();
    let typst = OutputSpec::Key {
        syntax: KeySyntax::Typst,
        command: None,
    };
    let named = |target: AppId| {
        let chosen = choose_with(&settings, Some(&target), Some(&typst));
        (chosen.id.unwrap_or_default(), chosen.from)
    };
    assert_eq!(named(app(Macos, "md.obsidian")), ("typst".into(), FormatFrom::Yours));
    // Nothing named is lost where Ibis cannot tell the app's kind.
    assert_eq!(
        named(app(Macos, "org.example.invented")),
        ("typst".into(), FormatFrom::Yours)
    );
    assert_eq!(named(app(Windows, "notepad.exe")), ("typst".into(), FormatFrom::Yours));
    // A word processor's documents stay formatted, as with your format.
    assert_eq!(
        named(app(Macos, "com.microsoft.word")),
        ("apa".into(), FormatFrom::Suggestion)
    );
}

#[test]
fn a_writing_chosen_for_this_session_answers_without_a_captured_app() {
    let typst = OutputSpec::Key {
        syntax: KeySyntax::Typst,
        command: None,
    };
    let writing = WritingName {
        id: "session".into(),
        name: "Invented notes".into(),
    };
    let chosen = for_writing(&typst, writing.clone(), None);
    assert_eq!(
        (chosen.id.as_deref(), chosen.from, chosen.writing),
        (Some("typst"), FormatFrom::Writing, Some(writing))
    );
}

#[test]
fn a_first_successful_suggested_paste_is_reported_once_per_kind_and_survives_reopen()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.json");
    let word = app(Macos, "com.microsoft.word");
    let markdown = app(Macos, "md.obsidian");
    let mut settings = Settings::default();
    let first = choose(&settings, Some(&word));
    assert!(accept_suggestion(&mut settings, Some(&word), &first));
    assert_eq!(choose(&settings, Some(&word)).from, FormatFrom::App);
    let pages = app(Macos, "com.apple.iwork.pages");
    let next = choose(&settings, Some(&pages));
    assert!(!accept_suggestion(&mut settings, Some(&pages), &next));
    assert_eq!(choose(&settings, Some(&pages)).from, FormatFrom::App);
    crate::settings::Owner::open(path.clone())
        .change(|current| {
            *current = settings.clone();
            Ok::<_, crate::settings::SettingsError>(())
        })
        .map(|_| ())?;
    let mut settings = crate::settings::load(&path).settings;
    let keys = choose(&settings, Some(&markdown));
    assert!(accept_suggestion(&mut settings, Some(&markdown), &keys));
    assert!(!accept_suggestion(&mut settings, Some(&markdown), &keys));
    Ok(())
}

#[test]
fn a_delayed_paste_never_overwrites_a_newer_format_or_reports_the_old_suggestion() {
    let word = app(Macos, "com.microsoft.word");
    let mut settings = Settings::default();
    let old = choose(&settings, Some(&word));
    settings.remember_app_output(&word, "modern-language-association".into());
    let before = settings.clone();
    assert!(!accept_suggestion(&mut settings, Some(&word), &old));
    assert_eq!(settings, before);
    assert_eq!(usual(&settings, Some(&word)).from, FormatFrom::Suggestion);
}
