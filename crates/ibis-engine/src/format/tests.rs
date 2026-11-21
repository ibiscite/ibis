use super::*;

use crate::settings::writings::Writing;
use ibis_core::destination::System::{Macos, Windows};

fn app(system: ibis_core::destination::System, id: &str) -> AppId {
    AppId { system, id: id.into() }
}

/// The output id and the level that chose it, as the header reads them.
fn said(settings: &Settings, target: Option<&AppId>) -> (String, FormatFrom) {
    let chosen = choose(settings, target);
    (chosen.id.unwrap_or_default(), chosen.from)
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
        // Word and LibreOffice keep formatted citations on every system Ibis lists.
        (app(Windows, "winword.exe"), "apa", FormatKind::Documents),
        (app(Macos, "org.libreoffice.script"), "apa", FormatKind::Documents),
        (app(Windows, "soffice.exe"), "apa", FormatKind::Documents),
        // Key formats come first: an app Ibis does not list takes keys.
        (app(Macos, "org.example.invented"), "pandoc", FormatKind::Keys),
        // Listed apps of unknown kind count as documents: the native
        // journey pastes into Notepad and copies for a terminal.
        (app(Windows, "notepad.exe"), "apa", FormatKind::Documents),
        (app(Macos, "com.apple.terminal"), "apa", FormatKind::Documents),
    ] {
        let chosen = choose(&settings, Some(&target));
        assert_eq!(
            (chosen.id.as_deref(), chosen.from, chosen.kind),
            (Some(id), FormatFrom::Suggestion, kind),
            "{}",
            target.id
        );
    }
    // With no app captured (Linux copies for any app), documents.
    assert_eq!(said(&settings, None), ("apa".into(), FormatFrom::Suggestion));
    // Settings still shows the style for documents.
    assert_eq!(documents(&settings).id.as_deref(), Some("apa"));
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

#[test]
fn projects_are_offered_once_an_app_has_a_second_format_or_one_exists() {
    let word = app(Macos, "com.microsoft.word");
    let mut settings = Settings::default();
    assert!(!offers_projects(&settings, Some(&word)) && !offers_projects(&settings, None));
    // The first paste saves the suggestion as Word's format: still one format.
    let suggested = choose(&settings, Some(&word));
    assert!(accept_suggestion(&mut settings, Some(&word), &suggested));
    assert!(!offers_projects(&settings, Some(&word)));
    // A second one, chosen in Word, beside your usual APA.
    // An id this Ibis does not list is no format, as `choose` reads it.
    settings.remember_app_output(&word, "invented-output".into());
    assert!(!offers_projects(&settings, Some(&word)));
    settings.remember_app_output(&word, "modern-language-association".into());
    assert!(offers_projects(&settings, Some(&word)));
    assert!(!offers_projects(&settings, Some(&app(Macos, "md.obsidian"))));
    // A saved project is never hidden, in any app or none.
    let mut settings = Settings::default();
    settings.device.writings.push(Writing {
        name: "Thesis".into(),
        format: OutputSpec::Key {
            syntax: KeySyntax::Pandoc,
            command: None,
        },
        bibliography: None,
    });
    assert!(offers_projects(&settings, None));
}
