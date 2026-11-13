//! Invented reference through app output, restart, native preview and canonical copy.
use super::*;
use crate::library::engine_file::{FileLibrary, FileSelection};
use ibis_core::contract::OutputSpec;
use ibis_core::destination::System;
use serde_json::json;

#[test]
fn app_outputs_and_remembered_key_workflows_copy_the_exact_preview() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    fs::write(
        &path,
        "@article{demo,title={Synthetic research},author={Researcher, Synthetic},year={2026}}\n",
    )?;
    let file = FileLibrary::open(&path).map_err(|_| "open invented library")?;
    let rows = serde_json::to_value(
        file.search(
            &serde_json::from_value(json!({"text":"research","interpret_locators":false}))?,
            10,
            None,
            &ibis_engine::Cancellation::default(),
        )
        .map_err(|_| "search invented library")?,
    )?;
    let row = &rows["results"][0];
    let browser = AppId {
        system: System::Macos,
        id: "example.browser".into(),
    };
    let settings_path = directory.path().join("settings.json");
    let settings_file = crate::preferences::SettingsFile::new(settings_path.clone());
    let word = AppId {
        system: System::Macos,
        id: "com.microsoft.Word".into(),
    };
    let tex = AppId {
        system: System::Macos,
        id: "TeXShop".into(),
    };
    let defaults = ibis_engine::settings::Settings::default();
    let suggested = |app: &AppId| {
        ibis_engine::format::choose(&defaults, Some(app))
            .id
            .and_then(|id| crate::output::listed(&id))
    };
    let mut cases = vec![
        (suggested(&word).ok_or("Word output")?, "(Researcher, 2026)".to_owned()),
        (suggested(&tex).ok_or("TeX output")?, "\\autocite{demo}".to_owned()),
    ];
    // Every key output the engine lists, remembered, writes its own example;
    // Zotero's markers write only for references read from Zotero.
    cases.extend(ibis_engine::outputs::outputs().iter().filter_map(|output| {
        let example = output
            .example
            .as_ref()
            .filter(|_| matches!(output.output, OutputSpec::Key { .. }) && !output.zotero_items)?;
        Some((crate::output::listed(&output.id)?, example.replace("key", "demo")))
    }));
    assert!(cases.len() > 10, "every key output");
    for (output, expected) in cases {
        settings_file
            .update(|settings| settings.remember_app_output(&browser, output.id().to_owned()))
            .map_err(|error| format!("remember {}: {error}", output.id()))?;
        // What a restart reads for the app.
        let reopened = ibis_engine::settings::load(&settings_path).settings;
        let remembered = reopened
            .app_output(&browser)
            .and_then(crate::output::listed)
            .ok_or("remembered output")?;
        assert_eq!(remembered, output);
        let selection: FileSelection =
            serde_json::from_value(json!({"id":row["id"],"cite":row["cite"],"locator":null}))?;
        let preview = file
            .preview(file.reserve_preview(), vec![selection], remembered.id())
            .map_err(|_| "native preview")?;
        assert_eq!(preview.text, expected, "{output:?}");
        // APA's HTML for this fixture equals its text; key outputs have no HTML.
        let expected_html = matches!(output.spec(), OutputSpec::Csl { .. }).then_some(expected.as_str());
        let mut writes = 0;
        let copied = file
            .copy(&preview.ticket, &preview.content_hash, |content| {
                writes += 1;
                assert_eq!(content.text, preview.text);
                assert_eq!(content.html, expected_html, "{output:?}");
                Ok(())
            })
            .map_err(|_| "canonical copy")?;
        assert_eq!(writes, 1);
        assert_eq!(copied.text, preview.text);
    }
    Ok(())
}
