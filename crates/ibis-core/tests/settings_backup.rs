//! Taking settings to another computer (#1561): portable paths, shortcuts per
//! system, and backups that are checked whole before anything changes.
use std::fs;
use std::path::{Path, PathBuf};

use ibis_core::cite::KeyFormat;
use ibis_core::destination::System;
use ibis_core::settings::backup::{self, BackupError, Note};
use ibis_core::settings::{
    self, CitationStyle, Density, LibrarySetting, PickerWindow, Settings, SettingsError, ZoteroLibrary,
};
use serde_json::{Value, json};

fn folder() -> tempfile::TempDir {
    tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"))
}

fn write(path: &Path, text: &str) {
    fs::write(path, text).unwrap_or_else(|error| panic!("{error}"));
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn other_system() -> System {
    if System::HERE == System::Windows {
        System::Macos
    } else {
        System::Windows
    }
}

fn system_name(system: System) -> String {
    serde_json::to_value(system)
        .unwrap_or_default()
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn chosen() -> Settings {
    let mut settings = Settings {
        library: Some(LibrarySetting::Zotero(ZoteroLibrary::Group(42).into())),
        copy_format: Some(KeyFormat::Typst),
        citation_style: Some(CitationStyle::Ieee),
        density: Some(Density::Compact),
        update_checks: Some(true),
        ..Settings::default()
    };
    settings.shortcut.set_here(Some("Alt+Shift+K".into()));
    settings.picker_windows.insert(
        "display-1".into(),
        PickerWindow {
            width: 640,
            height: 420,
            custom_size: true,
            x: 10,
            y: 20,
        },
    );
    settings.unknown.insert("accent".into(), json!("purple"));
    settings
}

#[test]
fn home_paths_are_written_from_the_home_folder_and_read_on_any_computer() {
    let home = if cfg!(windows) {
        Path::new(r"C:\Users\Invented")
    } else {
        Path::new("/home/invented")
    };
    let inside = home.join("Documents").join("Thesis").join("refs.bib");
    assert_eq!(
        settings::portable_path(&inside, Some(home)).as_deref(),
        Some("~/Documents/Thesis/refs.bib")
    );
    assert_eq!(
        settings::resolve_path("~/Documents/Thesis/refs.bib", Some(home)),
        inside
    );
    // Another computer's home folder holds the same file under another name.
    let elsewhere = if cfg!(windows) {
        Path::new(r"D:\Profiles\other")
    } else {
        Path::new("/Users/other")
    };
    assert_eq!(
        settings::resolve_path("~/Documents/Thesis/refs.bib", Some(elsewhere)),
        elsewhere.join("Documents").join("Thesis").join("refs.bib")
    );
    // Outside the home folder, the home folder itself, and no home folder: as written.
    let outside = if cfg!(windows) {
        PathBuf::from(r"E:\Shared\refs.bib")
    } else {
        PathBuf::from("/srv/shared/refs.bib")
    };
    assert_eq!(settings::portable_path(&outside, Some(home)), None);
    assert_eq!(settings::portable_path(home, Some(home)), None);
    assert_eq!(settings::portable_path(&inside, None), None);
    assert_eq!(settings::resolve_path("~/refs.bib", None), PathBuf::from("~/refs.bib"));
    assert_eq!(
        settings::resolve_path("~other/refs.bib", Some(home)),
        PathBuf::from("~other/refs.bib")
    );
    assert_eq!(
        settings::resolve_path("~/a//b.bib", Some(home)),
        home.join("a").join("b.bib")
    );
}

#[test]
fn a_library_in_the_home_folder_is_saved_with_a_tilde_and_reopens_as_the_same_file() {
    let Some(home) = std::env::home_dir() else { return };
    let directory = folder();
    let path = directory.path().join("settings.json");
    let library = home.join("Invented Folder").join("refs.bib");
    settings::update(&path, |s| s.library = Some(LibrarySetting::Bibtex(library.clone())))
        .unwrap_or_else(|error| panic!("{error}"));
    let saved: Value = serde_json::from_str(&read(&path)).unwrap_or_default();
    assert_eq!(saved["library"]["bibtex"], "~/Invented Folder/refs.bib");
    assert_eq!(
        settings::load(&path).settings.library,
        Some(LibrarySetting::Bibtex(library))
    );
    // A full path written by hand is kept as it was.
    write(&path, r#"{"library":{"bibtex":"/srv/invented/refs.bib"}}"#);
    settings::update(&path, |s| s.larger_text = Some(true)).unwrap_or_else(|error| panic!("{error}"));
    let saved: Value = serde_json::from_str(&read(&path)).unwrap_or_default();
    assert_eq!(saved["library"]["bibtex"], "/srv/invented/refs.bib");
}

#[test]
fn shortcuts_are_kept_and_written_per_system() {
    let directory = folder();
    let path = directory.path().join("settings.json");
    let here = system_name(System::HERE);
    let other = system_name(other_system());
    write(&path, r#"{"shortcut":"Alt+Space"}"#);
    assert_eq!(settings::load(&path).settings.shortcut.here(), Some("Alt+Space"));
    write(
        &path,
        &json!({"shortcut": {other.clone(): "Control+Command+R"}}).to_string(),
    );
    let loaded = settings::load(&path);
    assert!(loaded.problem.is_none());
    assert_eq!(
        loaded.settings.shortcut.here(),
        None,
        "another system's shortcut is not this one's"
    );
    // Choosing this system's shortcut keeps the other system's.
    settings::update(&path, |s| s.shortcut.set_here(Some("Alt+K".into()))).unwrap_or_else(|error| panic!("{error}"));
    let saved: Value = serde_json::from_str(&read(&path)).unwrap_or_default();
    assert_eq!(
        saved["shortcut"],
        json!({here.clone(): "Alt+K", other.clone(): "Control+Command+R"})
    );
    // Back to the default here: the other system's stays.
    settings::update(&path, |s| s.shortcut.set_here(None)).unwrap_or_else(|error| panic!("{error}"));
    let saved: Value = serde_json::from_str(&read(&path)).unwrap_or_default();
    assert_eq!(saved["shortcut"], json!({other: "Control+Command+R"}));
    // Only this system's: still written per system, so a shared file never
    // hands it to another system.
    write(&path, "{}");
    settings::update(&path, |s| s.shortcut.set_here(Some("Alt+K".into()))).unwrap_or_else(|error| panic!("{error}"));
    let saved: Value = serde_json::from_str(&read(&path)).unwrap_or_default();
    assert_eq!(saved["shortcut"], json!({here: "Alt+K"}));
    // A system Ibis does not know makes the file invalid, as any wrong value does.
    write(&path, r#"{"shortcut":{"amiga":"Alt+K"}}"#);
    assert!(matches!(settings::load(&path).problem, Some(SettingsError::Invalid(_))));
}

#[test]
#[cfg(unix)]
fn a_settings_file_linked_into_a_synced_folder_stays_linked() {
    let directory = folder();
    let synced = directory.path().join("Cloud Drive");
    fs::create_dir(&synced).unwrap_or_else(|error| panic!("{error}"));
    let target = synced.join("settings.json");
    write(&target, r#"{"paste":false}"#);
    let path = directory.path().join("settings.json");
    std::os::unix::fs::symlink(&target, &path).unwrap_or_else(|error| panic!("{error}"));
    settings::update(&path, |s| s.larger_text = Some(true)).unwrap_or_else(|error| panic!("{error}"));
    assert!(fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()));
    let saved = settings::load(&target).settings;
    assert_eq!((saved.paste, saved.larger_text), (Some(false), Some(true)));
    assert_eq!(read(&directory.path().join("settings.json.bak")), r#"{"paste":false}"#);
    // No temporary file is left in either folder.
    for folder in [directory.path(), synced.as_path()] {
        let names: Vec<_> = fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert!(
            names.iter().all(|name| !name.to_string_lossy().contains(".tmp")),
            "{names:?}"
        );
    }
}

#[test]
fn sync_conflict_copies_are_named_and_ordinary_files_are_not() {
    let directory = folder();
    let path = directory.path().join("settings.json");
    write(&path, "{}");
    for name in [
        "settings 2.json",
        "settings-INVENTED-PC.json",
        "settings (Invented's conflicted copy 2026-11-02).json",
        "settings (1).json",
        "settings.sync-conflict-20261102-101500-ABCDEFG.json",
        "settings.json.bak",
        "settings.json.invalid-1",
        "settings.json.tmp-1",
        "apps.json",
        "settings 2.txt",
        "settings (copy).json",
        "settings-x.y.json",
    ] {
        write(&directory.path().join(name), "{}");
    }
    assert_eq!(
        settings::sync_conflicts(&path),
        [
            "settings (1).json",
            "settings (Invented's conflicted copy 2026-11-02).json",
            "settings 2.json",
            "settings-INVENTED-PC.json",
            "settings.sync-conflict-20261102-101500-ABCDEFG.json",
        ]
    );
    let empty = folder();
    assert!(settings::sync_conflicts(&empty.path().join("settings.json")).is_empty());
}

#[test]
fn a_backup_holds_what_travels_and_names_the_system_of_each_shortcut() {
    let text = backup::export(&chosen(), "0.2.8").unwrap_or_else(|error| panic!("{error}"));
    let written: Value = serde_json::from_str(&text).unwrap_or_default();
    assert_eq!(written["ibis_settings_backup"], 1);
    assert_eq!(written["made_by"], "0.2.8");
    assert_eq!(written["made_on"], system_name(System::HERE));
    let settings = &written["settings"];
    for local in backup::LOCAL {
        assert!(settings.get(local).is_none(), "{local} stays on this computer");
    }
    assert_eq!(settings["shortcut"], json!({system_name(System::HERE): "Alt+Shift+K"}));
    assert_eq!(settings["library"], json!({"zotero": {"group": 42}}));
    assert_eq!(settings["accent"], "purple");
    let read = backup::parse(&text).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(read.made_on, System::HERE);
    assert_eq!(read.settings.citation_style, Some(CitationStyle::Ieee));
    assert_eq!(read.settings.update_checks, None);
    assert!(read.settings.picker_windows.is_empty());
}

#[test]
fn damaged_foreign_and_newer_backups_are_refused_with_their_reason() {
    let good: Value = serde_json::from_str(&backup::export(&chosen(), "0.2.8").unwrap_or_default()).unwrap_or_default();
    let with = |change: &dyn Fn(&mut Value)| {
        let mut text = good.clone();
        change(&mut text);
        backup::parse(&text.to_string())
    };
    assert!(matches!(backup::parse("{"), Err(BackupError::NotABackup)));
    assert!(matches!(backup::parse("[]"), Err(BackupError::NotABackup)));
    assert!(matches!(
        backup::parse(r#"{"title":"Invented"}"#),
        Err(BackupError::NotABackup)
    ));
    assert!(matches!(
        backup::parse(r#"{"paste":false}"#),
        Err(BackupError::SettingsFile)
    ));
    assert!(matches!(
        backup::parse(r#"{"schema_version":2}"#),
        Err(BackupError::SettingsFile)
    ));
    assert!(matches!(
        with(&|b| b["ibis_settings_backup"] = json!(2)),
        Err(BackupError::NewerFormat(2))
    ));
    assert!(matches!(
        with(&|b| b["ibis_settings_backup"] = json!("1")),
        Err(BackupError::Invalid(_))
    ));
    assert!(matches!(
        with(&|b| b["made_on"] = json!("amiga")),
        Err(BackupError::Invalid(_))
    ));
    assert!(matches!(
        with(&|b| b["made_by"] = json!(7)),
        Err(BackupError::Invalid(_))
    ));
    assert!(matches!(
        with(&|b| b["extra"] = json!(true)),
        Err(BackupError::Invalid(_))
    ));
    assert!(matches!(
        with(&|b| b["settings"] = json!([])),
        Err(BackupError::Invalid(_))
    ));
    assert!(matches!(
        with(&|b| b["settings"]["schema_version"] = json!(3)),
        Err(BackupError::Settings(SettingsError::NewerVersion(3)))
    ));
    assert!(matches!(
        with(&|b| b["settings"]["density"] = json!("roomy")),
        Err(BackupError::Settings(SettingsError::Invalid(_)))
    ));
    let large = format!("{{\"ibis_settings_backup\":1,\"pad\":\"{}\"}}", " ".repeat(1024 * 1024));
    assert!(matches!(backup::parse(&large), Err(BackupError::Invalid(_))));
    // A shortcut written as text belongs to the system that made the backup.
    let text = with(&|b| {
        b["made_on"] = json!(system_name(other_system()));
        b["settings"]["shortcut"] = json!("Control+Command+R");
    })
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(text.settings.shortcut.here(), None);
    assert_eq!(
        text.settings
            .shortcut
            .by_system()
            .get(&other_system())
            .map(String::as_str),
        Some("Control+Command+R")
    );
}

#[test]
fn a_backup_file_that_is_missing_or_not_downloaded_is_reported_and_read_whole_otherwise() {
    let directory = folder();
    let missing = backup::read(&directory.path().join("Ibis settings.json"));
    assert!(matches!(missing, Err(BackupError::Unreadable(error)) if error.kind() == std::io::ErrorKind::NotFound));
    let placeholder = directory.path().join(".Ibis settings.json.icloud");
    write(&placeholder, "bplist00");
    assert!(matches!(backup::read(&placeholder), Err(BackupError::NotDownloaded)));
    assert!(matches!(
        backup::read(directory.path()),
        Err(BackupError::Unreadable(_))
    ));
    let file = directory.path().join("Ibis settings.json");
    write(&file, &backup::export(&chosen(), "0.2.8").unwrap_or_default());
    assert!(backup::read(&file).is_ok());
    fs::write(&file, [0xff, 0xfe]).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(backup::read(&file), Err(BackupError::Invalid(_))));
}

#[test]
fn restoring_on_another_computer_keeps_its_local_settings_and_shortcut() {
    let made_elsewhere = {
        let mut text: Value =
            serde_json::from_str(&backup::export(&chosen(), "0.2.8").unwrap_or_default()).unwrap_or_default();
        text["made_on"] = json!(system_name(other_system()));
        text["settings"]["shortcut"] = json!({system_name(other_system()): "Control+Command+R"});
        backup::parse(&text.to_string()).unwrap_or_else(|error| panic!("{error}"))
    };
    let mut here = Settings {
        update_checks: Some(false),
        density: Some(Density::Spacious),
        ..Settings::default()
    };
    here.shortcut.set_here(Some("Alt+Space".into()));
    here.picker_windows.insert(
        "display-9".into(),
        PickerWindow {
            width: 500,
            height: 400,
            custom_size: false,
            x: 0,
            y: 0,
        },
    );
    let plan = backup::plan(&here, &made_elsewhere, |_| true);
    assert_eq!(
        plan.restored.update_checks,
        Some(false),
        "the update question stays this computer's"
    );
    assert_eq!(plan.restored.picker_windows, here.picker_windows);
    assert_eq!(plan.restored.shortcut.here(), Some("Alt+Space"));
    assert_eq!(
        plan.restored
            .shortcut
            .by_system()
            .get(&other_system())
            .map(String::as_str),
        Some("Control+Command+R")
    );
    assert_eq!(plan.notes, [Note::ShortcutKept]);
    let changed: Vec<_> = plan.changes.iter().map(|change| change.setting.as_str()).collect();
    let other_shortcut = format!("shortcut.{}", system_name(other_system()));
    let mut expected = vec![
        "accent",
        "citation_style",
        "copy_format",
        "density",
        "library",
        other_shortcut.as_str(),
    ];
    expected.sort_unstable();
    assert_eq!(changed, expected);
    let density = plan.changes.iter().find(|change| change.setting == "density");
    assert_eq!(
        density.map(|change| (change.from.clone(), change.to.clone())),
        Some((Some(json!("spacious")), Some(json!("compact"))))
    );
}

#[test]
fn a_bibliography_missing_on_this_computer_is_restored_and_noted() {
    let mut chosen = chosen();
    chosen.library = Some(LibrarySetting::Bibtex(PathBuf::from("/invented/elsewhere/refs.bib")));
    let backup =
        backup::parse(&backup::export(&chosen, "0.2.8").unwrap_or_default()).unwrap_or_else(|error| panic!("{error}"));
    let plan = backup::plan(&Settings::default(), &backup, |_| false);
    assert_eq!(plan.notes, [Note::LibraryNotFound]);
    assert_eq!(plan.restored.library, chosen.library);
}

#[test]
fn restore_saves_the_backup_and_keeps_the_settings_it_replaced() {
    let directory = folder();
    let path = directory.path().join("settings.json");
    let original =
        r#"{"paste":false,"picker_windows":{"display-1":{"width":500,"height":400,"custom_size":false,"x":1,"y":2}}}"#;
    write(&path, original);
    let backup = backup::parse(&backup::export(&chosen(), "0.2.8").unwrap_or_default())
        .unwrap_or_else(|error| panic!("{error}"));
    let plan = backup::plan(&settings::load(&path).settings, &backup, |_| true);
    let restored = backup::restore(&path, &plan).unwrap_or_else(|error| panic!("{error}"));
    // Close and reopen: the restored settings are what loads.
    let reopened = settings::load(&path);
    assert!(reopened.problem.is_none());
    assert_eq!(reopened.settings, restored);
    assert_eq!(reopened.settings.citation_style, Some(CitationStyle::Ieee));
    assert_eq!(
        reopened.settings.paste, None,
        "the backup's choices replace this computer's"
    );
    assert_eq!(reopened.settings.picker_windows.len(), 1, "window places stay");
    assert_eq!(read(&directory.path().join("settings.json.bak")), original);
    // Restoring the same backup again changes nothing and writes nothing.
    let again = backup::plan(&reopened.settings, &backup, |_| true);
    assert!(again.is_empty());
    let before = read(&path);
    backup::restore(&path, &again).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(read(&path), before);
    assert_eq!(read(&directory.path().join("settings.json.bak")), original);
}

#[test]
fn a_restore_shown_before_another_change_is_refused_and_changes_nothing() {
    let directory = folder();
    let path = directory.path().join("settings.json");
    write(&path, r#"{"density":"spacious"}"#);
    let backup = backup::parse(&backup::export(&chosen(), "0.2.8").unwrap_or_default())
        .unwrap_or_else(|error| panic!("{error}"));
    let plan = backup::plan(&settings::load(&path).settings, &backup, |_| true);
    // Another window changes a setting after the restore was shown.
    settings::update(&path, |s| s.larger_text = Some(true)).unwrap_or_else(|error| panic!("{error}"));
    let changed = read(&path);
    assert!(matches!(backup::restore(&path, &plan), Err(BackupError::Changed)));
    assert_eq!(read(&path), changed);
}

#[test]
fn restore_replaces_damaged_settings_but_never_newer_or_unreadable_ones() {
    let backup = backup::parse(&backup::export(&chosen(), "0.2.8").unwrap_or_default())
        .unwrap_or_else(|error| panic!("{error}"));
    // Damaged with no backup: defaults are shown, and the damaged bytes are kept.
    let directory = folder();
    let path = directory.path().join("settings.json");
    write(&path, "{ damaged");
    let plan = backup::plan(&settings::load(&path).settings, &backup, |_| true);
    backup::restore(&path, &plan).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(settings::load(&path).settings.citation_style, Some(CitationStyle::Ieee));
    assert_eq!(read(&directory.path().join("settings.json.invalid-1")), "{ damaged");
    // Damaged, recovered from a .bak that already matches the backup: still
    // worth restoring, because it repairs the damaged file.
    let directory = folder();
    let path = directory.path().join("settings.json");
    write(&path, "{ damaged");
    write(
        &directory.path().join("settings.json.bak"),
        &serde_json::to_string(&backup.settings).unwrap_or_default(),
    );
    let loaded = settings::load(&path);
    let plan = backup::plan(&loaded.settings, &backup, |_| true).repairing(backup::is_damaged(loaded.problem.as_ref()));
    assert!(plan.changes.is_empty());
    assert!(!plan.is_empty());
    assert!(plan.notes.contains(&backup::Note::RepairsSettings));
    backup::restore(&path, &plan).unwrap_or_else(|error| panic!("{error}"));
    assert!(settings::load(&path).problem.is_none());
    assert_eq!(read(&directory.path().join("settings.json.invalid-1")), "{ damaged");
    // No settings file yet: the restore creates it.
    let directory = folder();
    let path = directory.path().join("nested").join("settings.json");
    let plan = backup::plan(&settings::load(&path).settings, &backup, |_| true);
    backup::restore(&path, &plan).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(settings::load(&path).settings.density, Some(Density::Compact));
    // From a newer Ibis: refused and left as it is.
    let directory = folder();
    let path = directory.path().join("settings.json");
    write(&path, r#"{"schema_version":99}"#);
    let plan = backup::plan(&settings::load(&path).settings, &backup, |_| true);
    assert!(matches!(
        backup::restore(&path, &plan),
        Err(BackupError::Settings(SettingsError::NewerVersion(99)))
    ));
    assert_eq!(read(&path), r#"{"schema_version":99}"#);
}

#[test]
fn a_restored_file_cannot_turn_on_update_checks_or_carry_window_places() {
    let mut text: Value =
        serde_json::from_str(&backup::export(&Settings::default(), "0.2.8").unwrap_or_default()).unwrap_or_default();
    text["settings"]["update_checks"] = json!(true);
    text["settings"]["picker_windows"] = json!({"display-1": {"width":1,"height":1,"custom_size":true,"x":0,"y":0}});
    text["settings"]["delights"] = json!({"paste_count": 999});
    let backup = backup::parse(&text.to_string()).unwrap_or_else(|error| panic!("{error}"));
    let plan = backup::plan(&Settings::default(), &backup, |_| true);
    assert_eq!(plan.restored, Settings::default());
    assert!(plan.is_empty());
}

#[test]
fn an_interrupted_backup_never_blocks_the_next_one() {
    let directory = folder();
    let path = directory.path().join("Ibis settings.json");
    write(&directory.path().join(".Ibis settings.json.partial"), "half");
    write(
        &directory
            .path()
            .join(format!(".Ibis settings.json.{}-0.partial", std::process::id())),
        "half",
    );
    backup::save(&path, "{}\n", false).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(read(&path), "{}\n");
    backup::save(&path, "{ }\n", true).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(read(&path), "{ }\n");
    assert!(
        matches!(backup::save(&path, "{}\n", false), Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists)
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or_default();
        assert_eq!(mode, 0o600, "a backup names a private path, so only its owner reads it");
    }
}

#[test]
fn a_shared_settings_file_keeps_each_systems_shortcut_apart() {
    let directory = folder();
    let path = directory.path().join("settings.json");
    settings::update(&path, |s| s.shortcut.set_here(Some("Alt+Space".into())))
        .unwrap_or_else(|error| panic!("{error}"));
    let written: Value = serde_json::from_str(&read(&path)).unwrap_or_default();
    let here = serde_json::to_value(ibis_core::destination::System::HERE).unwrap_or_default();
    let here = here.as_str().unwrap_or_default();
    assert_eq!(
        written["shortcut"],
        json!({ here: "Alt+Space" }),
        "never a plain string another system would take as its own"
    );
}
