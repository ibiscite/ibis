use super::*;
use ibis_core::contract::KeySyntax;
use ibis_core::destination::System::{Macos, Windows};
use ibis_core::settings::writing_projects::WritingProject;

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
        output: Some("chicago-author-date".into()),
        ..Settings::default()
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
        key_output: Some("typst".into()),
        ..settings
    };
    assert_eq!(said(&settings, Some(&obsidian)), ("typst".into(), FormatFrom::Yours));
    assert_eq!(
        said(&settings, Some(&word)),
        ("chicago-author-date".into(), FormatFrom::Yours)
    );
    // A key id left in the documents value, or a style in the key value, is no choice.
    let crossed = Settings {
        output: Some("pandoc".into()),
        key_output: Some("apa".into()),
        ..Settings::default()
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
        key_output: Some("typst".into()),
        ..Settings::default()
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
    let thesis: WritingProject = serde_json::from_value(serde_json::json!({
        "id": "thesis", "revision": "r1", "name": "Thesis",
        "output": {"kind": "csl", "style": "chicago-notes-bibliography"},
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    settings.writing_projects.push(thesis);
    settings
        .writing_project_apps
        .insert("macos:com.microsoft.word".into(), "thesis".into());
    let chosen = choose(&settings, Some(&word));
    assert_eq!(chosen.from, FormatFrom::Writing);
    assert_eq!(chosen.id.as_deref(), Some("chicago-notes-bibliography"));
    assert_eq!(
        chosen.writing,
        Some(WritingName {
            id: "thesis".into(),
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
