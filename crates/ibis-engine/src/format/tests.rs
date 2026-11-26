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
    assert!(!accept_suggestion(&mut settings, Some(&word), None, &old));
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
    assert!(accept_suggestion(&mut settings, Some(&word), None, &suggested));
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

fn site(address: &str) -> AppId {
    ibis_core::destination::site(address).unwrap_or_else(|| panic!("{address} is a site"))
}

fn at(settings: &Settings, app: &AppId, site: Option<&AppId>) -> (String, FormatFrom, FormatKind) {
    let chosen = choose_at(settings, Some(app), site);
    (chosen.id.unwrap_or_default(), chosen.from, chosen.kind)
}

#[test]
fn in_a_browser_each_listed_site_gets_its_own_kind_whatever_the_browser_remembers() {
    let chrome = app(Macos, "com.google.chrome");
    let overleaf = site("https://www.overleaf.com/project/0000");
    let docs = site("https://docs.google.com/document/d/invented/edit");
    let mut settings = Settings::default();
    // A suggestion accepted in Chrome before sites were read.
    settings.remember_app_output(&chrome, "apa".into());
    assert_eq!(
        at(&settings, &chrome, Some(&overleaf)),
        ("autocite".into(), FormatFrom::Suggestion, FormatKind::Keys)
    );
    assert_eq!(
        at(&settings, &chrome, Some(&docs)),
        ("apa".into(), FormatFrom::Suggestion, FormatKind::Documents)
    );
    assert_eq!(
        usual_at(&settings, Some(&chrome), Some(&overleaf)).id.as_deref(),
        Some("autocite")
    );
    // The site cannot be read: the browser's own format, as before.
    assert_eq!(
        at(&settings, &chrome, None),
        ("apa".into(), FormatFrom::App, FormatKind::Documents)
    );
}

#[test]
fn a_format_chosen_on_a_site_stays_with_that_site() {
    let chrome = app(Windows, "chrome.exe");
    let overleaf = site("overleaf.com/project/0000");
    let docs = site("https://docs.google.com/");
    let mut settings = Settings::default();
    let owner = remembers_for(Some(&chrome), Some(&overleaf));
    assert_eq!(owner.as_ref(), Some(&overleaf));
    settings.remember_app_output(&overleaf, "natbib".into());
    assert_eq!(
        at(&settings, &chrome, Some(&overleaf)),
        ("natbib".into(), FormatFrom::App, FormatKind::Keys)
    );
    // Google Docs keeps its style, and so does Chrome itself.
    assert_eq!(at(&settings, &chrome, Some(&docs)).1, FormatFrom::Suggestion);
    assert_eq!(at(&settings, &chrome, None).1, FormatFrom::Suggestion);
    // The same site in another browser, or a subdomain, is the same site.
    let safari = app(Macos, "com.apple.safari");
    assert_eq!(
        at(&settings, &safari, Some(&site("https://eu.overleaf.com/x"))).0,
        "natbib"
    );
    assert!(offers_projects_at(&settings, Some(&chrome), Some(&overleaf)));
    forget_at(&mut settings, Some(&chrome), Some(&overleaf));
    assert!(settings.device.app_outputs.is_empty());
    assert_eq!(at(&settings, &chrome, Some(&overleaf)).1, FormatFrom::Suggestion);
}

#[test]
fn unlisted_sites_use_only_the_browsers_remembered_format() {
    let firefox = app(Macos, "org.mozilla.firefox");
    let wiki = ibis_core::destination::site("https://wiki.example.org/page?id=0");
    let other = ibis_core::destination::site("https://notes.example.net/");
    let mut settings = Settings::default();
    settings.remember_app_output(&firefox, "ieee".into());
    assert_eq!(
        at(&settings, &firefox, wiki.as_ref()),
        ("ieee".into(), FormatFrom::App, FormatKind::Documents)
    );
    let owner = remembers_for(Some(&firefox), wiki.as_ref());
    assert_eq!(owner.as_ref(), Some(&firefox));
    settings.remember_app_output(&firefox, "modern-language-association".into());
    assert_eq!(at(&settings, &firefox, wiki.as_ref()).0, "modern-language-association");
    assert_eq!(at(&settings, &firefox, other.as_ref()).0, "modern-language-association");
    assert_eq!(remembered_for(&settings, Some(&firefox), wiki.as_ref()), Some(&firefox));
    assert_eq!(settings.device.app_outputs.len(), 1);
    assert_eq!(settings.device.app_outputs[0].app, firefox);
    forget_at(&mut settings, Some(&firefox), other.as_ref());
    assert!(settings.device.app_outputs.is_empty());
    let suggestion = choose_at(&settings, Some(&firefox), wiki.as_ref());
    assert_eq!(suggestion.from, FormatFrom::Suggestion);
    assert!(accept_suggestion(
        &mut settings,
        Some(&firefox),
        wiki.as_ref(),
        &suggestion
    ));
    assert_eq!(settings.device.app_outputs.len(), 1);
    assert_eq!(settings.device.app_outputs[0].app, firefox);
}

#[test]
fn a_site_chooses_only_inside_a_browser_and_never_selects_a_project() {
    let word = app(Macos, "com.microsoft.word");
    let chrome = app(Macos, "com.google.chrome");
    let overleaf = site("https://overleaf.com/");
    let settings = Settings::default();
    // A site reported for an app that is not a browser is ignored.
    assert_eq!(at(&settings, &word, Some(&overleaf)).0, "apa");
    assert_eq!(remembers_for(Some(&word), Some(&overleaf)).as_ref(), Some(&word));
    // Projects bind to the browser; the site never picks one.
    let mut settings = Settings::default();
    let thesis: Writing = serde_json::from_value(serde_json::json!({
        "name": "Thesis",
        "format": {"kind": "csl", "style": "chicago-notes-bibliography"},
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    settings.device.writings.push(thesis);
    settings
        .device
        .writing_apps
        .insert(crate::app_context::app_key(&chrome), "Thesis".into());
    let chosen = choose_at(&settings, Some(&chrome), Some(&overleaf));
    assert_eq!(chosen.from, FormatFrom::Writing);
    settings.device.writing_apps.clear();
    settings
        .device
        .writing_apps
        .insert(crate::app_context::app_key(&overleaf), "Thesis".into());
    assert_eq!(
        choose_at(&settings, Some(&chrome), Some(&overleaf)).from,
        FormatFrom::Suggestion
    );
    // Nor when a request names the site itself.
    assert_eq!(choose(&settings, Some(&overleaf)).from, FormatFrom::Suggestion);
}

#[test]
fn a_site_named_by_a_request_resolves_like_an_app() {
    // `--app overleaf.com` from the command line or an agent.
    let overleaf = AppId::named("overleaf.com", Macos);
    assert_eq!(overleaf.system, ibis_core::destination::System::Web);
    assert_eq!(said(&Settings::default(), Some(&overleaf)).0, "autocite");
    assert_eq!(
        said(&Settings::default(), Some(&AppId::named("typst.app", Windows))).0,
        "typst"
    );
}

#[test]
fn a_website_entry_holding_anything_but_a_site_name_is_dropped_when_read() {
    let device: crate::settings::Device = serde_json::from_value(serde_json::json!({
        "app_outputs": [
            {"app": {"system": "web", "id": "https://example.org/private?token=invented"}, "output": "ieee"},
            {"app": {"system": "web", "id": "example.org"}, "output": "ieee"},
            {"app": {"system": "web", "id": "invented-person.notes.example.org"}, "output": "ieee"},
            {"app": {"system": "web", "id": "overleaf.com.example.org"}, "output": "ieee"},
            {"app": {"system": "web", "id": "www.overleaf.com"}, "output": "natbib"},
            {"app": {"system": "web", "id": "overleaf.com"}, "output": "autocite"},
            {"app": {"system": "macos", "id": "com.google.chrome"}, "output": "apa"},
        ]
    }))
    .unwrap_or_else(|error| panic!("{error}"));
    let kept: Vec<_> = device.app_outputs.iter().map(|entry| entry.app.id.as_str()).collect();
    assert_eq!(kept, ["overleaf.com", "com.google.chrome"]);
}

#[test]
fn an_unlisted_site_name_cannot_be_remembered_directly() {
    let mut settings = Settings::default();
    for host in [
        "example.org",
        "invented-person.notes.example.org",
        "overleaf.com.example.org",
    ] {
        let id = app(ibis_core::destination::System::Web, host);
        settings.remember_app_output(&id, "ieee".into());
        assert_eq!(settings.app_output(&id), None);
        assert!(settings.device.app_outputs.is_empty());
    }
}
