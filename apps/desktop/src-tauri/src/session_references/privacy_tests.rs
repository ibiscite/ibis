use super::*;
use ibis_core::contract::ItemRef;
use tauri::Emitter;

#[test]
fn native_disable_retires_delayed_recording_and_uncertain_settings_cannot_enable_it()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.json");
    let mut settings = ibis_core::settings::Settings::default();
    ibis_core::settings::save(&path, &settings)?;
    let app = tauri::test::mock_builder()
        .manage(SessionReferences::default())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build()?;
    manage(app.handle(), &path, true);
    let history = app.state::<SessionReferences>();
    let id = ItemRef {
        library: ibis_core::contract::LibraryId::new("invented")?,
        key: "a".into(),
    };
    let epoch = history.lock().epoch();
    history.record(&epoch, Some("invented.writer"), std::slice::from_ref(&id));
    history.flush()?;
    let recent_path = dir.path().join("recents.json");
    std::fs::remove_file(&recent_path)?;
    std::fs::create_dir(&recent_path)?;
    settings.recent_citations = Some(false);
    ibis_core::settings::save(&path, &settings)?;
    window.emit(
        "preferences:changed",
        serde_json::json!({"fields":["recent_citations"],"recent_citations":false}),
    )?;
    assert!(!history.lock().has_history());
    assert!(history.lock().forget_failed);
    std::fs::remove_dir(&recent_path)?;
    history.forget()?;
    // A forged enable event and an uncertain actual store cannot override off.
    std::fs::write(&path, "invalid invented settings")?;
    std::fs::remove_file(path.with_extension("json.bak"))?;
    window.emit(
        "preferences:changed",
        serde_json::json!({"fields":["recent_citations"],"recent_citations":true}),
    )?;
    let current = history.lock().epoch();
    history.record(&current, Some("invented.writer"), std::slice::from_ref(&id));
    history.flush()?;
    assert!(!history.lock().has_history());
    std::fs::remove_file(&path)?;
    settings.recent_citations = Some(true);
    ibis_core::settings::save(&path, &settings)?;
    window.emit(
        "preferences:changed",
        serde_json::json!({"fields":["recent_citations"],"recent_citations":true}),
    )?;
    history.record(&epoch, Some("invented.writer"), std::slice::from_ref(&id));
    history.flush()?;
    assert!(!history.lock().has_history());
    // A delayed off notification must forget, but cannot override a newer saved on.
    let old = history.lock().epoch();
    window.emit(
        "preferences:changed",
        serde_json::json!({"fields":["recent_citations"],"recent_citations":false}),
    )?;
    history.record(&old, Some("invented.writer"), std::slice::from_ref(&id));
    history.flush()?;
    assert!(!history.lock().has_history());
    let fresh = history.lock().epoch();
    history.record(&fresh, Some("invented.writer"), std::slice::from_ref(&id));
    history.flush()?;
    // An older save of another field, its snapshot still showing off, forgets nothing.
    window.emit(
        "preferences:changed",
        serde_json::json!({"fields":["density"],"recent_citations":false}),
    )?;
    assert_eq!(history.lock().ids(Some("invented.writer")), [id]);
    Ok(())
}
