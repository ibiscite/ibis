use super::*;
use ibis_core::destination::System;

fn mac(id: &str) -> AppId {
    AppId {
        system: System::Macos,
        id: id.into(),
    }
}

fn word() -> AppId {
    mac("com.microsoft.word")
}

/// An invented program location digest, as the desktop's capture makes one.
fn identity(seed: char) -> Option<String> {
    Some(seed.to_string().repeat(64))
}

fn grants_in(directory: &tempfile::TempDir) -> Grants {
    Grants::beside(&directory.path().join("settings.json"))
}

#[test]
fn the_plugin_note_shows_once_per_app_and_survives_a_restart() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let grants = grants_in(&directory);
    assert!(grants.take_plugin_note(&word()));
    assert!(!grants.take_plugin_note(&word()));
    assert!(grants.take_plugin_note(&mac("com.apple.iwork.pages")), "per app");
    assert!(!grants_in(&directory).take_plugin_note(&word()));
    Ok(())
}

#[test]
fn another_executable_with_the_same_basename_does_not_inherit_paste() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let grants = grants_in(&directory);
    let first = AppId {
        system: System::Windows,
        id: r"C:\Invented\Writer\editor.exe".into(),
    };
    let other = AppId {
        system: System::Windows,
        id: r"C:\Invented\Viewer\editor.exe".into(),
    };
    grants.set_choice(&first, AppPaste::Paste, identity('a'))?;
    assert_eq!(
        grants.paste_for(&first, identity('a').as_deref()),
        Ok(Some(AppPaste::Paste))
    );
    // Both normalize to `editor.exe`; only the program's own identity differs.
    assert_eq!(grants.paste_for(&other, identity('b').as_deref()), Ok(None));
    Ok(())
}

#[test]
fn a_reused_mac_bundle_id_or_missing_identity_requires_new_approval() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let app = mac("org.example.writer");
    let grants = grants_in(&directory);
    assert!(grants.set_choice(&app, AppPaste::Paste, None).is_err());
    grants.set_choice(&app, AppPaste::Paste, identity('a'))?;
    let reopened = grants_in(&directory);
    assert_eq!(
        reopened.paste_for(&app, identity('a').as_deref()),
        Ok(Some(AppPaste::Paste))
    );
    assert_eq!(reopened.paste_for(&app, identity('b').as_deref()), Ok(None));
    assert_eq!(reopened.paste_for(&app, None), Ok(None));
    Ok(())
}

#[test]
fn no_refused_app_can_be_approved() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let grants = grants_in(&directory);
    for entry in ibis_core::destination::APPS
        .iter()
        .filter(|entry| entry.safety != PasteSafety::Normal)
    {
        let app = AppId {
            system: entry.system,
            id: entry.id.into(),
        };
        assert!(
            grants.set_choice(&app, AppPaste::Paste, identity('a')).is_err(),
            "{}",
            entry.name
        );
        assert_eq!(grants.paste_for(&app, identity('a').as_deref()), Ok(None));
    }
    Ok(())
}

#[test]
fn the_file_holds_only_app_ids_and_choices_and_keeps_the_latest_apps() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let grants = grants_in(&directory);
    for n in 0..=MAX_APPS {
        grants.set_choice(&mac(&format!("org.example.app{n}")), AppPaste::CopyOnly, None)?;
    }
    assert_eq!(
        grants.paste_for(&mac("org.example.app0"), None),
        Ok(None),
        "the oldest app is dropped"
    );
    assert_eq!(
        grants.paste_for(&mac(&format!("org.example.app{MAX_APPS}")), None),
        Ok(Some(AppPaste::CopyOnly))
    );
    let file: serde_json::Value = serde_json::from_str(&fs::read_to_string(directory.path().join("apps.json"))?)?;
    assert_eq!(file["apps"].as_array().map(Vec::len), Some(MAX_APPS));
    assert_eq!(
        file["apps"][0],
        serde_json::json!({ "app": { "system": "macos", "id": "org.example.app1" }, "paste": "copy-only" })
    );
    Ok(())
}

/// The file is the only copy: one process's choice is seen by the next answer
/// in another, and neither overwrites the other's apps.
#[test]
fn two_owners_of_one_file_keep_each_others_choices() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let (app, daemon) = (grants_in(&directory), grants_in(&directory));
    app.set_choice(&mac("org.example.one"), AppPaste::CopyOnly, None)?;
    daemon.set_choice(&mac("org.example.two"), AppPaste::Paste, identity('c'))?;
    assert_eq!(
        daemon.paste_for(&mac("org.example.one"), None),
        Ok(Some(AppPaste::CopyOnly))
    );
    assert_eq!(
        app.paste_for(&mac("org.example.two"), identity('c').as_deref()),
        Ok(Some(AppPaste::Paste))
    );
    let threads: Vec<_> = (0..8)
        .map(|n| {
            let grants = grants_in(&directory);
            std::thread::spawn(move || {
                grants.set_choice(&mac(&format!("org.example.many{n}")), AppPaste::CopyOnly, None)
            })
        })
        .collect();
    for thread in threads {
        thread.join().map_err(|_| io::Error::other("a writer panicked"))??;
    }
    for n in 0..8 {
        assert_eq!(
            app.paste_for(&mac(&format!("org.example.many{n}")), None),
            Ok(Some(AppPaste::CopyOnly))
        );
    }
    Ok(())
}

#[test]
fn an_unreadable_file_grants_nothing_until_each_app_is_chosen_again() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("apps.json");
    let grants = grants_in(&directory);
    let app = mac("org.example.writer");
    grants.set_choice(&app, AppPaste::CopyOnly, None)?;
    fs::write(&path, "not json")?;
    assert_eq!(grants.paste_for(&app, None), Err(CopyReason::PasteUnavailable));
    assert!(!grants.take_plugin_note(&word()));
    grants.set_choice(&app, AppPaste::Paste, identity('a'))?;
    assert_eq!(
        grants.paste_for(&app, identity('a').as_deref()),
        Ok(Some(AppPaste::Paste))
    );
    // Another app still needs its own answer.
    assert_eq!(
        grants.paste_for(&mac("org.example.other"), None),
        Err(CopyReason::PasteUnavailable)
    );
    Ok(())
}

#[test]
fn a_failed_or_oversized_save_keeps_the_previous_durable_choice() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("apps.json");
    let grants = grants_in(&directory);
    let browser = mac("example.browser");
    grants.set_choice(&browser, AppPaste::CopyOnly, None)?;
    let before = fs::read(&path)?;
    assert!(
        grants
            .set_choice(&mac(&"x".repeat(MAX_BYTES as usize)), AppPaste::CopyOnly, None)
            .is_err()
    );
    assert_eq!(fs::read(&path)?, before);
    assert_eq!(grants.paste_for(&browser, None), Ok(Some(AppPaste::CopyOnly)));
    // A directory at the destination refuses the replacement and grants nothing.
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert!(grants.set_choice(&browser, AppPaste::Paste, identity('a')).is_err());
    assert_eq!(grants.paste_for(&browser, None), Err(CopyReason::PasteUnavailable));
    Ok(())
}

#[test]
fn a_read_only_file_refuses_changes_and_keeps_its_choice() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("apps.json");
    let grants = grants_in(&directory);
    grants.set_choice(&word(), AppPaste::Paste, None)?;
    let mut permissions = fs::metadata(&path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions)?;
    let before = fs::read(&path)?;
    assert_eq!(
        grants
            .set_choice(&word(), AppPaste::CopyOnly, None)
            .err()
            .map(|error| error.kind()),
        Some(io::ErrorKind::PermissionDenied)
    );
    assert_eq!(grants.paste_for(&word(), None), Ok(Some(AppPaste::Paste)));
    assert_eq!(fs::read(&path)?, before);
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_existing_temporary_symlink_is_never_truncated() -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("apps.json");
    let occupied = directory.path().join("occupied");
    fs::write(&occupied, "synthetic existing data")?;
    std::os::unix::fs::symlink(&occupied, path.with_extension("json.tmp-1"))?;
    grants_in(&directory).set_choice(&mac("example.browser"), AppPaste::CopyOnly, None)?;
    assert_eq!(fs::read_to_string(occupied)?, "synthetic existing data");
    Ok(())
}
