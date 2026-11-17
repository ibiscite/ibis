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
        (app(Macos, "org.example.invented"), "apa", FormatKind::Documents),
        // Listed apps of unknown kind count as documents too: the native
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
    assert_eq!(said(&settings, None), ("apa".into(), FormatFrom::Suggestion));
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
