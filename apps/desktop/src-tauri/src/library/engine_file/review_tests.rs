use super::*;
use std::sync::{Arc, Barrier};

#[test]
fn an_older_blocking_worker_finishing_last_cannot_retire_the_newer_preview() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic.bib");
    std::fs::write(
        &path,
        "@book{a,title={Synthetic},author={Researcher, Synthetic},year={2026}}",
    )?;
    let file = Arc::new(FileLibrary::open(&path).map_err(|_| "file")?);
    let rows = serde_json::to_value(
        file.search(
            &serde_json::from_value(json!({"text":"synthetic","interpret_locators":false}))?,
            10,
            None,
        )
        .map_err(|_| "search")?,
    )?;
    let selected = json!({"id":rows["results"][0]["id"],"cite":rows["results"][0]["cite"],"locator":null});
    // Native command entry reserves first; its blocking worker deliberately starts last.
    let older = file.reserve_preview();
    let started = Barrier::new(2);
    let release = Barrier::new(2);
    std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let old = scope.spawn(|| {
            started.wait();
            release.wait();
            file.preview(
                older,
                vec![serde_json::from_value(selected.clone()).unwrap_or_else(|e| panic!("{e}"))],
                KeyFormat::Pandoc,
                Some(CitationStyle::Apa),
            )
        });
        started.wait();
        let newer = file.reserve_preview();
        let ready = file
            .preview(
                newer,
                vec![serde_json::from_value(selected.clone())?],
                KeyFormat::Pandoc,
                None,
            )
            .map_err(|_| "newer preview")?;
        release.wait();
        assert!(
            old.join().map_err(|_| "old worker")?.is_err(),
            "older request must not replace the current native slot"
        );
        let copied = file
            .copy(&ready.ticket, &ready.content_hash, |content| {
                assert_eq!(content.text, "[@a]");
                Ok(())
            })
            .map_err(|_| "newer copy")?;
        assert_eq!(copied.text, "[@a]");
        Ok(())
    })?;
    Ok(())
}
