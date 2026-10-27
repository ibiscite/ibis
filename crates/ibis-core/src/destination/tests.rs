use super::*;
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
fn output_follows_the_app() {
    let default = OutputSpec::Key {
        syntax: KeySyntax::Typst,
        command: None,
    };
    let output = |app: AppId| output_for(Some(&app), &default, "apa");
    assert_eq!(
        output(mac("com.microsoft.Word")),
        OutputSpec::Csl { style: "apa".into() }
    );
    assert_eq!(output(windows("WINWORD.EXE")), OutputSpec::Csl { style: "apa".into() });
    assert_eq!(
        output(mac("md.obsidian")),
        OutputSpec::Key {
            syntax: KeySyntax::Pandoc,
            command: None
        }
    );
    assert_eq!(
        output(mac("TeXShop")),
        OutputSpec::Key {
            syntax: KeySyntax::Biblatex,
            command: None
        }
    );
    assert_eq!(output(mac("com.example.unknown")), default);
    assert_eq!(output(mac("com.apple.Terminal")), default);
    assert_eq!(output_for(None, &default, "apa"), default);
}

#[test]
fn an_ordinary_app_gets_the_paste() {
    assert_eq!(decide(&target(mac("com.microsoft.Word")), ON), Decision::Paste);
    assert_eq!(decide(&target(mac("com.example.unknown")), ON), Decision::Paste);
    assert_eq!(decide(&target(windows("obsidian.exe")), ON), Decision::Paste);
}

#[test]
fn the_paste_switch_wins_over_everything() {
    let off = PastePermission {
        enabled: false,
        granted: true,
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
