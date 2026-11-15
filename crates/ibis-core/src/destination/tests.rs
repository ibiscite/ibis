use super::*;

#[test]
fn zotero_finds_and_copies_even_when_an_old_rule_approves_paste() {
    for (system, id) in [(System::Macos, "org.zotero.zotero"), (System::Windows, "zotero.exe")] {
        let app = AppId { system, id: id.into() };
        assert!(finds_in_app(&app));
        let target = Target {
            system,
            app: Some(app),
            secure_field: false,
            elevated: false,
        };
        assert_eq!(
            decide(
                &target,
                PastePermission {
                    enabled: true,
                    granted: true,
                    app: Some(AppPaste::Paste)
                }
            ),
            Decision::Copy(CopyReason::CopyOnly)
        );
    }
}
use std::collections::HashSet;

fn mac(id: &str) -> AppId {
    AppId {
        system: System::Macos,
        id: id.into(),
    }
}

fn windows(id: &str) -> AppId {
    AppId {
        system: System::Windows,
        id: id.into(),
    }
}

fn target(app: AppId) -> Target {
    Target {
        system: app.system,
        app: Some(app),
        secure_field: false,
        elevated: false,
    }
}

const ON: PastePermission = PastePermission {
    enabled: true,
    granted: true,
    app: None,
};

#[test]
fn the_table_is_normalized_and_has_no_duplicates() {
    let mut seen = HashSet::new();
    for app in APPS {
        let normalized = AppId {
            system: app.system,
            id: app.id.into(),
        }
        .normalized();
        assert_eq!(app.id, normalized, "{} is stored normalized", app.name);
        assert!(!app.id.is_empty() && !app.name.is_empty());
        assert!(seen.insert((app.system, app.id)), "{} appears once", app.name);
        assert!(app.system != System::Linux, "Linux has no app IDs yet");
        if app.safety != PasteSafety::Normal {
            assert_eq!(app.output, None, "{} keeps the default output", app.name);
        }
    }
}

#[test]
fn app_ids_match_as_the_system_reports_them() {
    assert!(lookup(&mac("com.microsoft.Word")).is_some());
    assert!(lookup(&windows(r"C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE")).is_some());
    // The same ID on another system is another app.
    assert!(lookup(&windows("com.microsoft.Word")).is_none());
    assert!(lookup(&mac("com.example.unknown")).is_none());
}

#[test]
fn an_ordinary_app_gets_the_paste() {
    assert_eq!(decide(&target(mac("com.microsoft.Word")), ON), Decision::Paste);
    assert_eq!(decide(&target(windows("obsidian.exe")), ON), Decision::Paste);
}

#[test]
fn an_unrecognised_app_never_gets_a_blind_paste() {
    for app in [mac("org.example.remote"), windows("unknown-viewer.exe")] {
        for granted in [true, false] {
            let decision = decide(&target(app.clone()), PastePermission { granted, ..ON });
            assert_eq!(
                serde_json::to_value(decision).unwrap_or_default(),
                serde_json::json!({ "action": "copy", "reason": "unknown-app" })
            );
        }
    }
}

#[test]
fn an_explicit_app_paste_choice_allows_only_ordinary_targets() {
    let permission = serde_json::from_value::<PastePermission>(serde_json::json!({
        "enabled": true, "granted": true, "app": "paste"
    }));
    assert!(permission.is_ok(), "per-app paste approval is a separate permission");
    let permission = permission.unwrap_or(ON);
    assert_eq!(decide(&target(mac("org.example.writer")), permission), Decision::Paste);
    for app in APPS.iter().filter(|app| app.safety != PasteSafety::Normal) {
        let target = target(AppId {
            system: app.system,
            id: app.id.into(),
        });
        assert!(matches!(decide(&target, permission), Decision::Copy(_)), "{}", app.name);
    }
    assert_eq!(
        decide(
            &target(mac("org.example.writer")),
            PastePermission {
                granted: false,
                ..permission
            }
        ),
        Decision::Copy(CopyReason::NoPermission)
    );
}

#[test]
fn copy_only_is_separate_from_format_and_the_global_paste_switch() {
    for app in [
        mac("org.example.writer"),
        mac("com.microsoft.word"),
        windows("obsidian.exe"),
    ] {
        assert_eq!(
            decide(
                &target(app),
                PastePermission {
                    app: Some(AppPaste::CopyOnly),
                    ..ON
                }
            ),
            Decision::Copy(CopyReason::CopyOnly)
        );
    }
    assert_eq!(
        decide(
            &target(mac("org.example.writer")),
            PastePermission {
                enabled: false,
                app: Some(AppPaste::Paste),
                ..ON
            }
        ),
        Decision::Copy(CopyReason::PasteOff)
    );
}

#[test]
fn verified_writing_apps_paste_and_verified_viewers_and_editors_refuse() {
    for id in [
        "com.anthropic.claudefordesktop",
        "com.openai.codex",
        "com.openai.chat",
        "com.apple.Notes",
        "notion.id",
        "com.lukilabs.lukiapp",
        "com.microsoft.onenote.mac",
        "org.mozilla.thunderbird",
        "com.logseq.logseq",
    ] {
        assert_eq!(decide(&target(mac(id)), ON), Decision::Paste, "{id}");
    }
    for app in [
        mac("tv.parsec.www"),
        mac("com.p5sys.jump.mac.viewer"),
        mac("com.p5sys.jump.mac.viewer.web"),
        mac("com.nomachine.nxdock"),
        mac("com.omnissa.horizon.client.mac"),
        windows("parsecd.exe"),
        windows("nxplayer.exe"),
        windows("nxplayer.bin"),
        windows("vmware-view.exe"),
    ] {
        assert_eq!(
            decide(
                &target(app),
                PastePermission {
                    app: Some(AppPaste::Paste),
                    ..ON
                }
            ),
            Decision::Copy(CopyReason::RemoteDesktop)
        );
    }
    for id in ["emacs.exe", "gvim.exe", "neovide.exe"] {
        assert_eq!(
            decide(
                &target(windows(id)),
                PastePermission {
                    app: Some(AppPaste::Paste),
                    ..ON
                }
            ),
            Decision::Copy(CopyReason::NonstandardShortcut)
        );
    }
}

#[test]
fn the_paste_switch_wins_over_everything() {
    let off = PastePermission {
        enabled: false,
        granted: true,
        app: None,
    };
    let mut secure = target(mac("com.apple.Terminal"));
    secure.secure_field = true;
    assert_eq!(decide(&secure, off), Decision::Copy(CopyReason::PasteOff));
}

#[test]
fn linux_copies() {
    let linux = Target {
        system: System::Linux,
        app: None,
        secure_field: false,
        elevated: false,
    };
    assert_eq!(decide(&linux, ON), Decision::Copy(CopyReason::UnsupportedSystem));
}

#[test]
fn no_captured_app_copies() {
    let mut none = target(mac("com.microsoft.Word"));
    none.app = None;
    assert_eq!(decide(&none, ON), Decision::Copy(CopyReason::NoTarget));
    let mut mismatched = target(windows("winword.exe"));
    mismatched.system = System::Macos;
    assert_eq!(decide(&mismatched, ON), Decision::Copy(CopyReason::NoTarget));
}

#[test]
fn only_word_processors_get_the_plugin_note_and_only_by_app_id() {
    for app in [
        mac("com.microsoft.Word"),
        mac("com.apple.iWork.Pages"),
        mac("org.libreoffice.script"),
        windows("WINWORD.EXE"),
        windows("soffice.bin"),
    ] {
        assert!(has_citation_plugins(&app), "{app:?}");
    }
    for app in [
        mac("com.apple.Safari"),
        mac("com.google.Chrome"),
        mac("com.apple.mail"),
        mac("org.example.Unknown"),
        windows("chrome.exe"),
    ] {
        assert!(!has_citation_plugins(&app), "{app:?}");
    }
}

#[test]
fn password_fields_copy() {
    let mut secure = target(mac("com.microsoft.Word"));
    secure.secure_field = true;
    assert_eq!(decide(&secure, ON), Decision::Copy(CopyReason::SecureField));
}

#[test]
fn elevated_apps_copy() {
    let mut elevated = target(windows("winword.exe"));
    elevated.elevated = true;
    assert_eq!(decide(&elevated, ON), Decision::Copy(CopyReason::ElevatedApp));
}

#[test]
fn terminals_copy() {
    assert_eq!(
        decide(&target(mac("com.googlecode.iterm2")), ON),
        Decision::Copy(CopyReason::Terminal)
    );
    assert_eq!(
        decide(&target(windows("pwsh.exe")), ON),
        Decision::Copy(CopyReason::Terminal)
    );
}

#[test]
fn remote_desktops_copy() {
    assert_eq!(
        decide(&target(windows("mstsc.exe")), ON),
        Decision::Copy(CopyReason::RemoteDesktop)
    );
}

#[test]
fn virtual_machines_copy() {
    assert_eq!(
        decide(&target(mac("com.parallels.desktop.console")), ON),
        Decision::Copy(CopyReason::VirtualMachine)
    );
}

#[test]
fn a_missing_permission_copies() {
    let denied = PastePermission {
        enabled: true,
        granted: false,
        app: None,
    };
    assert_eq!(
        decide(&target(mac("com.microsoft.Word")), denied),
        Decision::Copy(CopyReason::NoPermission)
    );
    // An unsafe app still reports why it is unsafe, which is what to fix first.
    assert_eq!(
        decide(&target(mac("com.apple.Terminal")), denied),
        Decision::Copy(CopyReason::Terminal)
    );
}

#[test]
fn decisions_serialize_as_message_codes() -> Result<(), serde_json::Error> {
    assert_eq!(
        serde_json::to_value(Decision::Paste)?,
        serde_json::json!({"action": "paste"})
    );
    assert_eq!(
        serde_json::to_value(Decision::Copy(CopyReason::RemoteDesktop))?,
        serde_json::json!({"action": "copy", "reason": "remote-desktop"})
    );
    Ok(())
}

#[test]
fn an_id_belongs_to_the_system_whose_table_knows_it() {
    assert_eq!(AppId::named("WINWORD.EXE", System::Macos).system, System::Windows);
    assert_eq!(
        AppId::named("com.microsoft.Word", System::Windows).system,
        System::Macos
    );
    assert_eq!(
        AppId::named(r"C:\Office\WINWORD.EXE", System::Linux).system,
        System::Windows
    );
    assert_eq!(AppId::named("com.example.unknown", System::Linux).system, System::Linux);
}

#[test]
fn a_destination_says_what_the_app_gets_and_how() -> Result<(), serde_json::Error> {
    // The engine chooses the format (decision 0042); the destination carries it.
    let chosen = FormatChoice {
        output: OutputSpec::Csl { style: "ieee".into() },
        id: Some("ieee".into()),
        from: FormatFrom::Yours,
        kind: FormatKind::Documents,
        writing: None,
    };
    let at = |id: &str, running| destination(id, running, |_| chosen.clone());

    let word = at("com.microsoft.word", System::Macos);
    assert_eq!(word.name.as_deref(), Some("Microsoft Word"));
    assert_eq!(word.delivery, Decision::Paste);
    assert_eq!(word.output, OutputSpec::Csl { style: "ieee".into() });
    assert_eq!(word.from, FormatFrom::Yours);
    let mut newer = serde_json::to_value(&word)?;
    newer["added"] = serde_json::json!(true);
    newer["app"]["added"] = serde_json::json!(true);
    newer["output"]["added"] = serde_json::json!(true);
    assert_eq!(serde_json::from_value::<Destination>(newer)?, word);

    let terminal = at("com.apple.Terminal", System::Windows);
    assert_eq!(terminal.delivery, Decision::Copy(CopyReason::Terminal));

    for running in [System::Macos, System::Windows] {
        let unknown = at("com.example.unknown", running);
        assert_eq!(unknown.name, None);
        assert_eq!(unknown.delivery, Decision::Copy(CopyReason::UnknownApp));
    }
    let linux = at("com.example.unknown", System::Linux);
    assert_eq!(linux.delivery, Decision::Copy(CopyReason::UnsupportedSystem));
    Ok(())
}
