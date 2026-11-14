//! Defaults stay unset when another choice is saved.
#[path = "support/settings.rs"]
mod fixture;
use ibis_engine::settings;

#[test]
fn shortcut_defaults_stay_unwritten_and_explicit_choices_survive_saves() {
    for (shortcut, explicit_null) in [
        (None, false),
        (None, true),
        (Some("Control+Command+R"), false),
        (Some("control+command+r"), false),
        (Some("Alt+Shift+K"), false),
    ] {
        let (_directory, path) = fixture::file();
        let mut original = serde_json::json!({ "schema_version": settings::SCHEMA_VERSION });
        if shortcut.is_some() || explicit_null {
            original["shortcut"] = serde_json::json!(shortcut);
        }
        std::fs::write(&path, original.to_string()).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(fixture::read(&path).portable.shortcut.here(), shortcut);
        fixture::edit(&path, |settings| settings.portable.output = Some("typst".into()))
            .unwrap_or_else(|error| panic!("{error}"));
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap_or_default()).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(saved["schema_version"], 2);
        if let Some(shortcut) = shortcut {
            // Written per system, so a shared file keeps it to this one.
            let here = serde_json::to_value(ibis_core::destination::System::HERE).unwrap_or_default();
            assert_eq!(saved["shortcut"][here.as_str().unwrap_or_default()], shortcut);
        } else {
            assert!(saved.get("shortcut").is_none());
        }
    }
    let saved = serde_json::to_value(settings::Settings::default()).unwrap_or_default();
    assert!(saved.get("shortcut").is_none());
}
