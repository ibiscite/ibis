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
fn no_captured_app_copies() {
    let mut none = target(mac("com.microsoft.Word"));
    none.app = None;
    assert_eq!(decide(&none, ON), Decision::Copy(CopyReason::NoTarget));
    let mut mismatched = target(windows("winword.exe"));
    mismatched.system = System::Macos;
    assert_eq!(decide(&mismatched, ON), Decision::Copy(CopyReason::NoTarget));
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
