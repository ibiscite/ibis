//! Invented reference through app output, restart, native preview and canonical copy.
use super::*;
use crate::library::engine_file::{FileLibrary, FileSelection};
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
    let memory_path = directory.path().join("apps.json");
    let memory = AppMemory::open(memory_path.clone());
    let word = AppId {
        system: System::Macos,
        id: "com.microsoft.Word".into(),
    };
    let tex = AppId {
        system: System::Macos,
        id: "TeXShop".into(),
    };
    let default = crate::output::stored(None);
    let mut cases = vec![
        (automatic(&word, default).ok_or("Word output")?, "(Researcher, 2026)".to_owned()),
        (automatic(&tex, default).ok_or("TeX output")?, "\\autocite{demo}".to_owned()),
    ];
    // Every key output the engine lists, remembered, writes its own example.
    cases.extend(ibis_engine::outputs::outputs().iter().filter_map(|output| {
        let example = output.example.as_ref().filter(|_| matches!(output.output, OutputSpec::Key { .. }))?;
        Some((crate::output::listed(&output.id)?, example.replace("key", "demo")))
    }));
    assert!(cases.len() > 10, "every key output");
    for (output, expected) in cases {
        memory.set(&browser, Some(output))?;
        let reopened = AppMemory::open(memory_path.clone());
        assert_eq!(reopened.get(&browser), (Some(output), false));
        let remembered = reopened.get(&browser).0.ok_or("remembered output")?;
        let selection: FileSelection =
            serde_json::from_value(json!({"id":row["id"],"cite":row["cite"],"locator":null}))?;
        let preview = file
            .preview(file.reserve_preview(), vec![selection], remembered.id())
            .map_err(|_| "native preview")?;
        assert_eq!(preview.text, expected, "{output:?}");
        let mut writes = 0;
        let copied = file
            .copy(&preview.ticket, &preview.content_hash, |content| {
                writes += 1;
                assert_eq!(content.text, preview.text);
                assert_eq!(content.html, preview.html.as_deref());
                Ok(())
            })
            .map_err(|_| "canonical copy")?;
        assert_eq!(writes, 1);
        assert_eq!(copied.text, preview.text);
    }
    Ok(())
}
