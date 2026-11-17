use ibis_engine::settings::{
    Settings,
    writings::{self},
};

#[test]
fn unreadable_and_ambiguous_explicit_contexts_refuse_without_changing_settings() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.md");
    let mut settings = Settings::default();
    for name in ["Thesis", "Article"] {
        writings::create(
            &mut settings,
            name.into(),
            ibis_engine::outputs::find("apa").unwrap().output.clone(),
            None,
        )
        .unwrap();
    }
    let before = settings.clone();
    assert!(writings::from_context(&settings, Some(&missing), None, None).is_err());
    assert!(writings::from_context(&settings, None, None, Some("apa")).is_err());
    assert_eq!(settings, before);
}
