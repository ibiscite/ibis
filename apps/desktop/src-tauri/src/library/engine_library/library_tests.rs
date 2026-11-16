use super::*;
#[test]
fn shelf_recents_resolve_current_records_and_source_added_order_without_old_pages()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("synthetic-recents.json");
    std::fs::write(
        &path,
        r#"[{"id":"a","type":"book","title":"Synthetic A","date-added":"2026-10-01"},{"id":"b","type":"book","title":"Synthetic B","date-added":"2026-10-30"},{"id":"c","type":"book","title":"Synthetic C","date-added":"2026-10-15"}]"#,
    )?;
    let file = ibis_engine::library::Current::open(
        ibis_engine::Engine::new()
            .unwrap_or_else(|_| panic!("engine"))
            .with_formatting(crate::library::engine_library::FORMATTING.clone()),
        ibis_engine::library::Source::File(path.clone()),
        None,
        &ibis_engine::Cancellation::default(),
    )
    .map_err(|_| "open")?;
    let rows = file.search(
        &SearchQuery {
            text: "Synthetic A".into(),
            interpret_locators: true,
        },
        10,
        None,
        &Cancellation::default(),
    )?;
    let id: ItemRef = rows.results.first().ok_or("row")?.id.clone();
    let recent = serde_json::to_value(file.recents(&[id], true)?)?;
    let rows = recent["results"].as_array().ok_or("results")?;
    assert_eq!(
        rows.iter()
            .map(|row| row["title"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["Synthetic A", "Synthetic B", "Synthetic C"]
    );
    assert_eq!(rows[0]["recent"], "cited");
    assert_eq!(rows[1]["recent"], "added");
    assert!(rows.iter().all(|row| row.get("locator").is_none()));
    Ok(())
}
