use super::*;
use ibis_core::contract::{EngineError, ErrorCode};

fn loaded(name: &str, zotero: bool) -> LibraryStatus {
    let mut status = crate::library::snapshot_status(&Library::default());
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
        .chain(library.iter().copied())
        .chain(["—", "Settings…", "Check for Updates…", "Help", "—", "Quit Ibis Nightly"])
        .map(String::from)
        .collect()
}

#[test]
fn no_library_offers_a_file_and_only_offers_zotero_when_detected() {
    let status = crate::library::snapshot_status(&Library::default());
    let mut choices = LibraryChoices::default();
    assert_eq!(labels(&status, &choices), expected(&["No library yet", "Choose File…"]));
    choices.zotero_detected = true;
    assert_eq!(
        labels(&status, &choices),
        expected(&["No library yet", "Zotero", "Choose File…"])
    );
}

#[test]
fn a_loaded_zotero_library_shows_its_count_and_checked_choice() {
    let status = loaded("Zotero library", true);
    let source = Source::Zotero(ibis_core::settings::ZoteroLibrary::User.into());
    let mut choices = LibraryChoices::default();
    choices.observe(Some(&source), Some(&source));
    assert_eq!(
        labels(&status, &choices),
        expected(&["Zotero · 165 references", "Zotero", "Choose File…"])
    );
    assert!(choices.checked(Row::Zotero));
}

#[test]
fn a_loaded_file_is_named_and_checked_without_settings_toggles_or_reload() {
    let status = loaded("thesis.bib", false);
    let source = Source::File("thesis.bib".into());
    let mut choices = LibraryChoices::default();
    choices.observe(Some(&source), Some(&source));
    assert_eq!(
        labels(&status, &choices),
        expected(&["thesis.bib · 165 references", "thesis.bib", "Choose File…"])
    );
    assert!(choices.checked(Row::File));
}

#[test]
fn a_newer_zotero_library_moves_the_single_update_action_under_the_problem() {
    let mut status = loaded("Zotero library", true);
    status.state = LibraryState::Error;
    status.error_reason = Some(LoadError::Zotero(EngineError::new(ErrorCode::NewerFormat)));
    status.error = Some("wording is not a status code".into());
    let choices = LibraryChoices {
        zotero_detected: true,
        ..Default::default()
    };
    assert_eq!(
        labels(&status, &choices),
        vec![
            "Open Ibis",
            "—",
            "Zotero needs a newer Ibis",
            "Check for Updates…",
            "Zotero",
            "Choose File…",
            "—",
            "Settings…",
            "Help",
            "—",
            "Quit Ibis Nightly",
        ]
    );
}

#[test]
fn a_missing_file_moves_the_single_file_chooser_under_the_problem() {
    let mut status = loaded("thesis.bib", false);
    status.state = LibraryState::Error;
    status.error_reason = Some(LoadError::FILE_UNAVAILABLE);
    status.error = Some("wording is not a status code".into());
    let source = Source::File("thesis.bib".into());
    let mut choices = LibraryChoices::default();
    choices.observe(Some(&source), None);
    assert_eq!(
        labels(&status, &choices),
        expected(&["thesis.bib was moved or is unavailable", "Choose File…", "thesis.bib"])
    );
    assert!(!choices.checked(Row::File));
}

#[test]
fn a_failed_refresh_keeps_the_loaded_choice_and_offers_try_again() {
    let mut status = loaded("thesis.bib", false);
    status.stale = true;
    status.error = Some("wording is not a status code".into());
    status.error_reason = Some(LoadError::FileRefresh(EngineError::new(ErrorCode::BadInput)));
    let source = Source::File("thesis.bib".into());
    let mut choices = LibraryChoices::default();
    choices.observe(Some(&source), Some(&source));
    assert_eq!(
        labels(&status, &choices),
        expected(&[
            "thesis.bib could not be updated",
            "Try Again",
            "thesis.bib",
            "Choose File…"
        ])
    );
    assert!(choices.checked(Row::File));
}

#[test]
fn help_groups_shortcuts_news_feedback_reset_and_the_copyable_build() {
    let labels: Vec<_> = help_rows()
        .into_iter()
        .map(|row| {
            row.label(
                &crate::library::snapshot_status(&Library::default()),
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
fn switching_choices_moves_the_check_only_after_the_library_opens_and_retains_the_file() {
    let file = Source::File("thesis.bib".into());
    let zotero = Source::Zotero(ibis_core::settings::ZoteroLibrary::User.into());
    let mut choices = LibraryChoices::default();
    choices.observe(Some(&file), Some(&file));
    choices.observe(Some(&zotero), Some(&file));
    assert!(
        choices.checked(Row::File),
        "pending or refused Zotero keeps the file checked"
    );
    assert!(!choices.checked(Row::Zotero));
    choices.observe(Some(&zotero), Some(&zotero));
    assert!(choices.checked(Row::Zotero));
    assert!(!choices.checked(Row::File));
    assert_eq!(choices.file, Some("thesis.bib".into()));
    assert_eq!(choices.target(Row::File), Some(file.clone()));
    choices.observe(Some(&file), Some(&file));
    assert!(choices.checked(Row::File));
    assert!(!choices.checked(Row::Zotero));
}

#[test]
fn try_again_targets_the_refused_file_instead_of_the_retained_library() {
    let mut choices = LibraryChoices::default();
    let old = Source::File("old.bib".into());
    let requested = Source::File("thesis.bib".into());
    choices.observe(Some(&requested), Some(&old));
    assert_eq!(choices.target(Row::Retry), Some(requested));
    assert!(choices.checked(Row::File));
    assert_eq!(choices.target(Row::File), Some(old));
}

#[test]
fn a_failed_file_switch_keeps_the_loaded_file_checked_and_its_own_fix_despite_a_retained_refresh_error() {
    let old = Source::File("old.bib".into());
    let requested = Source::File("thesis.bib".into());
    let mut choices = LibraryChoices::default();
    choices.observe(Some(&requested), Some(&old));
    let mut status = loaded("thesis.bib", false);
    status.error = Some("wording is not a status code".into());
    status.error_reason = Some(LoadError::File(EngineError::new(ErrorCode::BadInput)));
    status.source_is_requested = false;
    status.stale = true;
    assert_eq!(
        labels(&status, &choices),
        expected(&[
            "thesis.bib does not contain valid references",
            "Choose File…",
            "old.bib"
        ])
    );
    assert!(choices.checked(Row::File));
    assert_eq!(choices.target(Row::File), Some(old));
    choices.observe(Some(&requested), Some(&requested));
    assert!(choices.checked(Row::File));
    assert_eq!(choices.target(Row::File), Some(requested));
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
    let targets = [(&id, &chosen)];
    assert!(
        library_target(&retired, targets).is_none(),
        "a queued click must not consume the new file choice"
    );
    let opened = library_target(&id, targets).ok_or("radio choice")?;
    let Source::File(path) = &opened else {
        return Err("file choice".into());
    };
    let file = crate::library::engine_file::FileLibrary::open(path).map_err(|e| e.to_string())?;
    let mut library = Library::default();
    library.file = Some(std::sync::Arc::new(file));
    library.source = Some(opened);
    let mut choices = LibraryChoices::default();
    choices.observe(library.requested.as_ref(), library.source.as_ref());
    assert!(choices.checked(Row::File));
    assert!(!choices.checked(Row::Zotero));
    assert_eq!(
        library_line(&crate::library::snapshot_status(&library)),
        "thesis.bib · 1 reference"
    );
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
