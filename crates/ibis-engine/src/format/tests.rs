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

fn render(chosen: &FormatChoice) -> Result<ibis_core::contract::CitationResult, ibis_core::contract::EngineError> {
    let mut case: ibis_core::contract::GoldenCase =
        serde_json::from_str(include_str!("../../../../fixtures/cases/02-pandoc.json"))
            .unwrap_or_else(|error| panic!("{error}"));
    case.request.output = chosen.output.clone();
    let engine = crate::Engine::from_snapshot(std::sync::Arc::new(case.library));
    engine.cite(&case.request, &crate::Cancellation::default())
}

fn cite(chosen: &FormatChoice) -> Result<String, Box<dyn std::error::Error>> {
    let result = render(chosen).map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        result.outcomes,
        [ibis_core::contract::ItemOutcome::Rendered { item: 0 }]
    );
    assert!(result.copy_content(1).is_some());
    Ok(result.text)
}

#[test]
fn a_bundled_writing_keeps_its_identity_and_beats_app_and_request_choices() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.json");
    let markdown = app(Macos, "md.obsidian");
    let output = OutputSpec::Csl {
        style: "abi-technik".into(),
    };
    let writing = WritingName {
        id: "Synthetic notes".into(),
        name: "Synthetic notes".into(),
    };
    crate::settings::Owner::open(path.clone()).change(|settings| {
        settings.remember_app_output(&markdown, "apa".into());
        settings.device.writings.push(Writing {
            name: writing.name.clone(),
            format: output.clone(),
            bibliography: None,
        });
        settings
            .device
            .writing_apps
            .insert("macos:md.obsidian".into(), writing.id.clone());
        Ok::<_, crate::settings::SettingsError>(())
    })?;
    let loaded = crate::settings::load(&path);
    let settings = loaded.cites().map_err(|error| error.to_string())?;
    let requested = OutputSpec::Key {
        syntax: KeySyntax::Typst,
        command: None,
    };
    for (chosen, kind) in [
        (
            choose_with(settings, Some(&markdown), Some(&requested)),
            FormatKind::Keys,
        ),
        (for_writing(&output, writing.clone(), None), FormatKind::Documents),
    ] {
        assert_eq!(chosen.output, output);
        assert_eq!(chosen.id.as_deref(), Some("abi-technik"));
        assert_eq!(chosen.from, FormatFrom::Writing);
        assert_eq!(chosen.kind, kind);
        assert_eq!(chosen.writing, Some(writing.clone()));
        assert_eq!(
            cite(&chosen)?,
            "Avery Taylor, Synthetic Signals (Imaginary Press, 2026)."
        );
    }
    Ok(())
}

#[test]
fn an_explicit_bundled_style_keeps_its_identity_where_document_formats_apply() -> Result<(), Box<dyn std::error::Error>>
{
    let mut settings = Settings::default();
    settings.portable.key_output = Some("typst".into());
    let output = OutputSpec::Csl {
        style: "abi-technik".into(),
    };
    for target in [
        None,
        Some(app(Macos, "com.microsoft.word")),
        Some(app(Macos, "org.example.invented")),
    ] {
        let chosen = choose_with(&settings, target.as_ref(), Some(&output));
        assert_eq!(chosen.output, output);
        assert_eq!(
            (chosen.id.as_deref(), chosen.from, chosen.kind),
            (Some("abi-technik"), FormatFrom::Yours, FormatKind::Documents)
        );
        assert_eq!(
            cite(&chosen)?,
            "Avery Taylor, Synthetic Signals (Imaginary Press, 2026)."
        );
    }
    let markdown = app(Macos, "md.obsidian");
    assert_eq!(
        choose_with(&settings, Some(&markdown), Some(&output)),
        choose(&settings, Some(&markdown))
    );
    Ok(())
}

#[test]
fn explicit_invalid_outputs_keep_their_request_and_reach_normal_refusal() {
    use ibis_core::contract::{EngineError, ErrorCode, ErrorReason};
    for (output, reason) in [
        (
            OutputSpec::Csl {
                style: "invented-unavailable-style".into(),
            },
            ErrorReason::UnknownStyle,
        ),
        (
            OutputSpec::Preset {
                name: "invented-unavailable-preset".into(),
            },
            ErrorReason::UnknownOutput,
        ),
        (
            OutputSpec::Key {
                syntax: KeySyntax::Biblatex,
                command: Some("invented-command".into()),
            },
            ErrorReason::UnknownOutput,
        ),
    ] {
        let writing = WritingName {
            id: "session".into(),
            name: "Synthetic notes".into(),
        };
        for chosen in [
            choose_with(&Settings::default(), None, Some(&output)),
            for_writing(&output, writing, None),
        ] {
            assert_eq!(chosen.output, output);
            assert_eq!(chosen.id, None);
            assert_eq!(render(&chosen), Err(EngineError::because(ErrorCode::BadInput, reason)));
        }
    }
}

#[test]
fn key_commands_and_presets_keep_their_exact_outputs_and_identities() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default();
    let autocite = OutputSpec::Key {
        syntax: KeySyntax::Biblatex,
        command: Some("autocite".into()),
    };
    let chosen = choose(&settings, Some(&app(Windows, "texstudio.exe")));
    assert_eq!(chosen.output, autocite);
    assert_eq!(cite(&chosen)?, "\\autocite{Example2026}");
    for output in [
        autocite,
        OutputSpec::Key {
            syntax: KeySyntax::Biblatex,
            command: None,
        },
        OutputSpec::Preset {
            name: "autocite".into(),
        },
    ] {
        let writing = WritingName {
            id: "session".into(),
            name: "Synthetic notes".into(),
        };
        for chosen in [
            choose_with(&settings, None, Some(&output)),
            for_writing(&output, writing, None),
        ] {
            assert_eq!(chosen.output, output);
            assert_eq!(chosen.id.as_deref(), Some("autocite"));
            assert_eq!(cite(&chosen)?, "\\autocite{Example2026}");
        }
    }
    Ok(())
}

#[test]
fn explicit_styles_match_the_pinned_node_oracle() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut item: serde_json::Value = serde_json::from_str(include_str!("../../../../fixtures/cases/02-pandoc.json"))?;
    item = item["library"]["objects"][0]["object"]["data"]["csl"].clone();
    item["id"] = serde_json::json!("ibis-0");
    // The existing oracle supports these styles; the noncurated style above
    // exercises the same selection routes with its exact standalone output.
    let styles = ["apa", "chicago-author-date", "modern-language-association"];
    let requests: Vec<_> = styles
        .iter()
        .map(|style| {
            serde_json::json!({
                "style": style, "items": [item], "cites": [{"id": "ibis-0"}],
                "bibliography": false, "keep_order": false, "locale": "en-US"
            })
        })
        .collect();
    let mut node = Command::new(std::env::var("IBIS_NODE").unwrap_or_else(|_| "node".into()))
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ibis-csl/tests/node-oracle.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    node.stdin
        .take()
        .ok_or("oracle stdin")?
        .write_all(&serde_json::to_vec(&requests)?)?;
    let expected = node.wait_with_output()?;
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&expected.stdout)?;
    assert_eq!(expected.len(), styles.len());
    for (style, expected) in styles.into_iter().zip(expected) {
        let chosen = choose_with(
            &Settings::default(),
            None,
            Some(&OutputSpec::Csl { style: style.into() }),
        );
        let actual = render(&chosen).map_err(|error| format!("{error:?}"))?;
        assert_eq!(actual.text, expected["text"].as_str().ok_or("oracle text")?);
        assert_eq!(actual.html.as_deref(), expected["html"].as_str());
        assert_eq!(actual.rtf.as_deref(), expected["rtf"].as_str());
    }
    Ok(())
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
    let mut settings = loaded.cites().map_err(|error| error.to_string())?.clone();
    for target in [
        None,
        Some(app(Macos, "com.microsoft.word")),
        Some(app(Macos, "org.example.invented")),
    ] {
        let chosen = choose(&settings, target.as_ref());
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
    let chosen = choose(&settings, Some(&markdown));
    assert_eq!(
        (chosen.id.as_deref(), chosen.from, chosen.kind),
        (Some("typst"), FormatFrom::Yours, FormatKind::Keys)
    );
    assert_eq!(cite(&chosen)?, "@Example2026");
    // A bundled style in the key setting is still no key choice.
    settings.portable.key_output = Some("abi-technik".into());
    let chosen = choose(&settings, Some(&markdown));
    assert_eq!(
        (chosen.id.as_deref(), chosen.from, chosen.kind),
        (Some("pandoc"), FormatFrom::Suggestion, FormatKind::Keys)
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
    let settings = loaded.cites().map_err(|error| error.to_string())?;
    let pandoc = OutputSpec::Key {
        syntax: KeySyntax::Pandoc,
        command: None,
    };
    let chosen = choose_with(settings, Some(&markdown), Some(&pandoc));
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
fn a_writing_chosen_for_the_app_beats_the_apps_format_and_says_its_name() {
    let word = app(Macos, "com.microsoft.word");
    let mut settings = Settings::default();
    settings.remember_app_output(&word, "apa".into());
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
    assert_eq!(chosen.from, FormatFrom::Writing);
    assert_eq!(chosen.id.as_deref(), Some("chicago-notes-bibliography"));
    assert_eq!(
        chosen.writing,
        Some(WritingName {
            id: "Thesis".into(),
            name: "Thesis".into()
        })
    );
    // Another app keeps its own answer.
    let pages = app(Macos, "com.apple.iwork.pages");
    assert_eq!(said(&settings, Some(&pages)), ("apa".into(), FormatFrom::Suggestion));
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
