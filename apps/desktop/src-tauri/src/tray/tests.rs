use super::*;

fn loaded(name: &str, zotero: bool) -> LibraryStatus {
    let mut status = crate::library::tests::owner(crate::preferences::SettingsFile::new(
        tempfile::tempdir().unwrap().path().join("settings.json"),
    ))
    .status();
    status.state = LibraryState::Loaded;
    status.name = Some(name.into());
    status.snapshot_name = status.name.clone();
    status.zotero = zotero;
    status.snapshot_zotero = zotero;
    status.has_snapshot = true;
    status.references = 165;
    status
}

fn labels(status: &LibraryStatus, choices: &LibraryChoices) -> Vec<String> {
    menu_rows(status, choices)
        .into_iter()
        .map(|row| row.label(status, choices, "Ibis Nightly"))
        .collect()
}

fn expected(library: &[&str]) -> Vec<String> {
    ["Open Ibis", "—"]
        .into_iter()
        .chain(["Library"])
        .chain(library.iter().copied())
        .chain(["—", "Settings…", "Check for Updates…", "Help", "—", "Quit Ibis Nightly"])
        .map(String::from)
        .collect()
}

#[test]
fn no_library_offers_a_file_and_only_offers_zotero_when_detected() {
    let status = crate::library::tests::owner(crate::preferences::SettingsFile::new(
        tempfile::tempdir().unwrap().path().join("settings.json"),
    ))
    .status();
    let mut choices = LibraryChoices::default();
    assert_eq!(labels(&status, &choices), expected(&["No library yet", "Choose File…"]));
    choices.zotero_detected = true;
    assert_eq!(
        labels(&status, &choices),
        expected(&["No library yet", "Zotero", "Choose File…"])
    );
}

#[test]
fn help_groups_shortcuts_news_feedback_reset_and_the_copyable_build() {
    let labels: Vec<_> = help_rows()
        .into_iter()
        .map(|row| {
            row.label(
                &crate::library::tests::owner(crate::preferences::SettingsFile::new(
                    tempfile::tempdir().unwrap().path().join("settings.json"),
                ))
                .status(),
                &LibraryChoices::default(),
                "Ibis Nightly",
            )
        })
        .collect();
    assert_eq!(
        labels,
        [
            "Keyboard Shortcuts",
            "What’s New",
            "—",
            "Report a Problem…",
            "Suggest an Idea…",
            "—",
            "Reset Window Position",
            "—",
            "Ibis Nightly"
        ]
    );
    assert_eq!(
        version_line("Ibis Nightly", "0.3.4-nightly.20261111"),
        "Ibis Nightly 0.3.4-nightly.20261111"
    );
}

#[test]
fn the_mac_key_equivalent_names_the_registered_key_on_its_layout_and_falls_back_when_unrepresentable() {
    use tauri_plugin_global_shortcut::{Code, Shortcut};
    let key = "Shift+Command+KeyQ"
        .parse::<Shortcut>()
        .unwrap_or_else(|e| panic!("{e}"));
    let (_, equivalent) = macos_shortcut(key, Some('a'));
    let equivalent = equivalent.and_then(|key| key.parse::<Shortcut>().ok());
    assert_eq!(
        equivalent.map(|key| key.key),
        Some(Code::KeyA),
        "physical Q types A on AZERTY"
    );
    let (label, equivalent) = macos_shortcut(key, Some('ı'));
    assert_eq!(label, "Shift+Command+ı");
    assert!(
        equivalent.is_none(),
        "Tauri's accelerator parser cannot represent this character"
    );
    assert!(
        macos_shortcut(key, None).1.is_none(),
        "an unresolved printable key must not claim its US letter"
    );
    let function = "Shift+Command+F4".parse::<Shortcut>().unwrap_or_else(|e| panic!("{e}"));
    assert!(macos_shortcut(function, None).1.is_some());
}

#[test]
fn a_file_radio_event_opens_its_exact_file_and_a_retired_event_cannot_open_the_replacement()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("thesis.bib");
    std::fs::write(&path, "@book{invented,title={An Invented Thesis}}")?;
    let chosen = Source::File(path);
    let id: tauri::menu::MenuId = "file-current".into();
    let retired: tauri::menu::MenuId = "file-retired".into();
    let action = LibraryAction::Open(chosen);
    let targets = [(&id, &action)];
    assert!(
        library_target(&retired, targets).is_none(),
        "a queued click must not consume the new file choice"
    );
    let LibraryAction::Open(opened) = library_target(&id, targets).ok_or("radio choice")? else {
        return Err("radio choice must open the named library".into());
    };
    let library = crate::library::tests::owner(crate::preferences::SettingsFile::new(
        directory.path().join("settings.json"),
    ));
    crate::library::tests::open(&library, opened);
    let mut choices = LibraryChoices::default();
    let view = library.view();
    choices.observe(view.requested.as_ref(), view.source());
    assert!(choices.checked(Row::File(0)));
    assert!(!choices.checked(Row::Zotero));
    assert_eq!(library_line(&library.status()), "thesis.bib · 1 reference");
    Ok(())
}

#[test]
fn windows_left_click_opens_once_and_right_click_is_left_to_the_native_menu() {
    use tauri::tray::{MouseButton, MouseButtonState};
    let click = |button, button_state| TrayIconEvent::Click {
        id: "ibis".into(),
        position: Default::default(),
        rect: Default::default(),
        button,
        button_state,
    };
    let mut opened = 0;
    for event in [
        click(MouseButton::Left, MouseButtonState::Down),
        click(MouseButton::Left, MouseButtonState::Up),
        click(MouseButton::Right, MouseButtonState::Down),
        click(MouseButton::Right, MouseButtonState::Up),
    ] {
        handle_tray_event(false, &event, || opened += 1);
    }
    assert_eq!(opened, 1);
    handle_tray_event(true, &click(MouseButton::Left, MouseButtonState::Up), || opened += 1);
    assert_eq!(opened, 1, "macOS left click only opens its menu");
    assert!(!menu_on_left_click(false));
    assert!(menu_on_left_click(true));
}

#[test]
fn recent_files_remain_one_click_choices_with_only_the_open_file_checked() {
    let mut choices = LibraryChoices::default();
    let old = Source::File("old.bib".into());
    let current = Source::File("thesis.bib".into());
    choices.observe(Some(&old), Some(&old));
    choices.observe(Some(&current), Some(&current));
    let status = loaded("thesis.bib", false);
    assert_eq!(
        labels(&status, &choices),
        expected(&["thesis.bib · 165 references", "thesis.bib", "old.bib", "Choose File…"])
    );
    assert!(choices.checked(Row::File(0)));
    assert!(!choices.checked(Row::File(1)));
    assert_eq!(choices.target(Row::File(1)), Some(old));
}

#[test]
fn reading_problems_offer_their_fix_instead_of_a_dead_count() {
    let mut status = loaded("thesis.bib", false);
    status.warnings = 2;
    let affected = ibis_core::contract::LibraryId::digest(b"invented warning library");
    status.problem_library = Some(affected.clone());
    assert!(matches!(
        library_action(Row::FixProblems, &status, &LibraryChoices::default()),
        Some(LibraryAction::FixProblems { library, .. }) if library == affected
    ));
    assert_eq!(
        labels(&status, &LibraryChoices::default()),
        expected(&["thesis.bib · 165 references", "Fix 2 Problems…", "Choose File…"])
    );
    status.warnings = 1;
    assert_eq!(
        labels(&status, &LibraryChoices::default()),
        expected(&["thesis.bib · 165 references", "Fix 1 Problem…", "Choose File…"])
    );
}

#[test]
fn spoken_state_names_reading_failure_and_readiness_without_reference_counts() {
    let mut status = crate::library::tests::owner(crate::preferences::SettingsFile::new(
        tempfile::tempdir().unwrap().path().join("settings.json"),
    ))
    .status();
    assert_eq!(spoken_title(&status), "Ibis, no library yet");
    status = loaded("thesis.bib", false);
    assert_eq!(spoken_title(&status), "Ibis, ready");
    for state in [LibraryState::Loading, LibraryState::Reloading] {
        status.state = state;
        assert_eq!(spoken_title(&status), "Ibis, reading thesis.bib");
    }
    status.state = LibraryState::Error;
    status.error = Some("unreadable".into());
    status.error_reason = Some(LoadError::NotLocal);
    assert_eq!(spoken_title(&status), "Ibis, thesis.bib could not be read");
    status = loaded("Zotero library", true);
    status.state = LibraryState::Loading;
    assert_eq!(spoken_title(&status), "Ibis, reading Zotero");
}
