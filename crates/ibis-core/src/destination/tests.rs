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

fn web(id: &str) -> Option<AppId> {
    Some(AppId {
        system: System::Web,
        id: id.into(),
    })
}

#[test]
fn unlisted_hosts_never_become_site_identities() {
    for host in [
        "example.org",
        "notes.example.org",
        "invented-person.notes.example.org",
        "www.notes.example.org",
        "private.example.co.uk",
        "xn--bcher-kva.example",
        "google.com",
        "unlisted.google.com",
        "overleaf.com.example.org",
        "notoverleaf.com",
    ] {
        assert_eq!(site(&format!("https://{host}/private?token=invented")), None, "{host}");
        let id = AppId {
            system: System::Web,
            id: host.into(),
        };
        assert_eq!(id.normalized(), "", "{host}");
        assert_eq!(lookup(&id), None, "{host}");
    }
    // Keep each listed host intact, including service labels such as docs.google.com.
    for entry in APPS.iter().filter(|entry| entry.system == System::Web) {
        assert_eq!(
            site(&format!("https://{}/private?token=invented", entry.id)),
            web(entry.id)
        );
        assert_eq!(site(&format!("https://eu.{}/private", entry.id)), web(entry.id));
    }
}

#[test]
fn a_site_is_only_its_name_never_a_path_query_or_sign_in() {
    for (address, kept) in [
        (
            "https://www.overleaf.com/project/0000?token=invented#main",
            web("overleaf.com"),
        ),
        ("HTTPS://EU.Overleaf.COM:443/read/x", web("overleaf.com")),
        // An address field may leave out the scheme.
        ("overleaf.com/project/0000", web("overleaf.com")),
        (
            "https://docs.google.com/document/d/invented/edit",
            web("docs.google.com"),
        ),
        ("https://mail.google.com/mail/u/0/#inbox", web("mail.google.com")),
        // Unlisted hosts have no site identity, whatever their address form.
        ("http://www.notes.example.org./a/b", None),
        ("https://wiki.example.net\\a", None),
        // A sign-in name could hide the real host.
        ("https://docs.google.com@evil.example/x?y", None),
        ("https://user:secret@example.org/", None),
        ("https://evil.example@docs.google.com/x?y", None),
        ("https://user:secret@overleaf.com/", None),
        // Not web pages, or not domain names.
        ("file:///Users/invented/thesis.tex", None),
        ("chrome://settings", None),
        ("about:blank", None),
        ("javascript:alert(1)", None),
        ("data:text/html,hello", None),
        ("http://127.0.0.1:8080/", None),
        ("http://[::1]/", None),
        ("http://localhost:3000/", None),
        ("https://example.org:port/", None),
        ("https://-bad-.example.org/", None),
        ("https://-bad-.overleaf.com/", None),
        ("https://bad..overleaf.com/", None),
        ("https://exa mple.org/", None),
        ("search words", None),
        ("", None),
    ] {
        assert_eq!(site(address), kept, "{address}");
    }
    let long = format!("https://{}.example.org/", "a".repeat(64));
    assert_eq!(site(&long), None);
    let deep = format!("https://{}example.org/", "a.".repeat(130));
    assert_eq!(site(&deep), None);
    // Listed suffixes still require valid DNS labels and a bounded host.
    let long = format!("https://{}.overleaf.com/", "a".repeat(64));
    assert_eq!(site(&long), None);
    let deep = format!("https://{}overleaf.com/", "a.".repeat(130));
    assert_eq!(site(&deep), None);
}

#[test]
fn a_site_id_that_is_not_a_site_name_compares_as_nothing() {
    let pasted = AppId {
        system: System::Web,
        id: "https://www.overleaf.com/project/0000".into(),
    };
    assert_eq!(pasted.normalized(), "");
    assert_eq!(lookup(&pasted), None);
    let sub = AppId {
        system: System::Web,
        id: "WWW.Overleaf.com".into(),
    };
    assert_eq!(sub.normalized(), "overleaf.com");
    assert_eq!(lookup(&sub).map(|entry| entry.name), Some("Overleaf"));
}

#[test]
fn the_table_is_normalized_unique_and_marks_only_apps_as_browsers() {
    let mut seen = std::collections::HashSet::new();
    for entry in APPS {
        let id = AppId {
            system: entry.system,
            id: entry.id.into(),
        };
        assert_eq!(id.normalized(), entry.id, "{} is stored normalized", entry.name);
        assert!(seen.insert((entry.system, entry.id)), "{} appears twice", entry.id);
        assert!(!(entry.browser && entry.system == System::Web), "{}", entry.id);
        assert!(entry.output.is_some() || entry.system != System::Web, "{}", entry.id);
    }
    assert!(is_browser(&mac("com.google.Chrome")));
    assert!(is_browser(&windows(r"C:\Program Files\Mozilla Firefox\firefox.exe")));
    assert!(!is_browser(&mac("md.obsidian")));
}

#[test]
fn the_tex_and_r_writing_apps_get_keys() {
    for (app, syntax) in [
        (mac("texstudio"), KeySyntax::Biblatex),
        (mac("com.vallettaventures.Texpad"), KeySyntax::Biblatex),
        (mac("com.rstudio.desktop"), KeySyntax::Pandoc),
        (windows("rstudio.exe"), KeySyntax::Pandoc),
        (mac("co.posit.positron"), KeySyntax::Pandoc),
        (windows("Positron.exe"), KeySyntax::Pandoc),
    ] {
        assert_eq!(
            lookup(&app).and_then(|entry| entry.output),
            Some(OutputClass::Key(syntax)),
            "{}",
            app.id
        );
        assert_eq!(decide(&target(app), ON), Decision::Paste);
    }
}
