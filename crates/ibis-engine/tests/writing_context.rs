#![allow(clippy::unwrap_used)]
use ibis_engine::settings::{
    Settings,
    writings::{self, Writing},
};

#[test]
fn explicit_markdown_frontmatter_selects_the_named_writing_and_other_documents_do_not() {
    let directory = tempfile::tempdir().unwrap();
    let bibliography = directory.path().join("refs.bib");
    let document = directory.path().join("chapter.qmd");
    std::fs::write(&bibliography, "@book{invented,title={Atlas}}\n").unwrap();
    std::fs::write(
        &document,
        "---\nbibliography: refs.bib\ncsl: apa.csl\n---\nInvented text\n",
    )
    .unwrap();
    let writing: Writing = serde_json::from_value(serde_json::json!({
        "name":"Thesis", "format":"apa", "bibliography":bibliography
    }))
    .unwrap();
    let settings = Settings {
        device: ibis_engine::settings::Device {
            writings: vec![writing.clone()],
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        writings::from_context(&settings, Some(&document), None, None).unwrap(),
        Some(writing)
    );
    let unrelated = directory.path().join("Thesis.md");
    std::fs::write(&unrelated, "A title and a nearby refs.bib are not a choice.").unwrap();
    assert!(
        writings::from_context(&settings, Some(&unrelated), None, None)
            .unwrap()
            .is_none()
    );
    assert!(writings::from_context(&settings, None, None, None).unwrap().is_none());
}

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
