//! The real native bridge over an invented raw Zotero endpoint, never discovery.
#[path = "../../../../../../crates/ibis-engine/tests/support/zotero.rs"]
mod fixture;
use super::{FileLibrary, FileSelection};
use ibis_core::{cite::KeyFormat, contract::CitationResult, settings::CitationStyle};
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::json;
use std::{
    io::Cursor,
    sync::{Arc, atomic::Ordering},
};

fn opened(server: &fixture::Server) -> Result<FileLibrary, Box<dyn std::error::Error>> {
    assert_ne!(server.port, ibis_zotero_local::DEFAULT_PORT);
    let engine = Engine::with_zotero_port(server.port).map_err(|_| "synthetic engine")?;
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-zotero"}),
            &Cancellation::default(),
        )
        .map_err(|_| "synthetic raw acquisition")?;
    Ok(FileLibrary::from_engine(engine))
}

fn selection(bridge: &FileLibrary) -> Result<FileSelection, Box<dyn std::error::Error>> {
    let query = serde_json::from_value(json!({"text":"atlas","interpret_locators":false}))?;
    let found = serde_json::to_value(
        bridge
            .search(&query, 10, None, &ibis_engine::Cancellation::default())
            .map_err(|_| "native search")?,
    )?;
    assert_eq!(found["results"].as_array().ok_or("results")?.len(), 1);
    let row = &found["results"][0];
    Ok(serde_json::from_value(
        json!({"id":row["id"],"cite":row["cite"],"locator":null}),
    )?)
}

#[test]
fn raw_zotero_picker_preview_and_copy_match_the_real_cli() -> Result<(), Box<dyn std::error::Error>> {
    let server = fixture::Server::new();
    let bridge = opened(&server)?;
    for (style, option, value) in [
        (None, "--format", "pandoc"),
        (Some(CitationStyle::Apa), "--style", "apa"),
    ] {
        let cli = Engine::with_zotero_port(server.port).map_err(|_| "synthetic CLI")?;
        let (mut output, mut errors) = (vec![], vec![]);
        assert_eq!(
            ibis::run_with(
                cli,
                ["ibis", "cite", "--zotero", "Synthetic2026", option, value, "--json"]
                    .map(Into::into)
                    .to_vec(),
                Cursor::new([]),
                &mut output,
                &mut errors,
                ibis::OutputMode::Plain
            ),
            0
        );
        assert!(errors.is_empty());
        let envelope: serde_json::Value = serde_json::from_slice(&output)?;
        let expected: CitationResult = serde_json::from_value(envelope["data"].clone())?;
        let preview = bridge
            .preview(
                bridge.reserve_preview(),
                vec![selection(&bridge)?],
                KeyFormat::Pandoc,
                style,
            )
            .map_err(|e| e.to_string())?;
        assert_eq!(preview.text, expected.text);
        assert_eq!(preview.html, expected.html);
        assert_eq!(preview.content_hash, expected.content_hash.as_str());
        let mut writes = 0;
        bridge
            .copy(&preview.ticket, &preview.content_hash, |content| {
                writes += 1;
                assert_eq!(content.text, expected.text);
                assert_eq!(content.html, expected.html.as_deref());
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        assert_eq!(writes, 1);
    }
    Ok(())
}

#[test]
fn native_zotero_copy_refuses_a_detected_switch_until_coherent_reload() -> Result<(), Box<dyn std::error::Error>> {
    let server = fixture::Server::new();
    let bridge = Arc::new(opened(&server)?);
    let preview = bridge
        .preview(
            bridge.reserve_preview(),
            vec![selection(&bridge)?],
            KeyFormat::Pandoc,
            None,
        )
        .map_err(|e| e.to_string())?;
    server.instance.store(2, Ordering::SeqCst);
    server.pause.store(true, Ordering::SeqCst);
    let reloading = Arc::clone(&bridge);
    let worker = std::thread::spawn(move || reloading.reload());
    server.wait_for_payload();
    let refused = bridge.copy(&preview.ticket, &preview.content_hash, |_| {
        panic!("a detected switch must not write")
    });
    server.pause.store(false, Ordering::SeqCst);
    worker
        .join()
        .map_err(|_| "reload worker")?
        .map_err(|_| "coherent reload")?;
    assert!(refused.is_err());
    assert!(
        bridge
            .copy(&preview.ticket, &preview.content_hash, |_| panic!(
                "an old receipt cannot revive"
            ))
            .is_err()
    );
    let current = bridge
        .preview(
            bridge.reserve_preview(),
            vec![selection(&bridge)?],
            KeyFormat::Pandoc,
            None,
        )
        .map_err(|e| e.to_string())?;
    let mut copied = String::new();
    bridge
        .copy(&current.ticket, &current.content_hash, |content| {
            copied = content.text.to_owned();
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    assert_eq!(copied, "[@Synthetic2026]");
    Ok(())
}

#[test]
fn chosen_groups_keep_source_identity_counts_and_scopes_consistent() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::settings::{ZoteroLibraries, ZoteroLibrary};
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).map_err(|_| "synthetic engine")?;
    let both = ZoteroLibraries::new(vec![ZoteroLibrary::Group(7), ZoteroLibrary::User]).ok_or("choice")?;
    let bridge = FileLibrary::open_zotero_with(engine, &both, &Cancellation::default()).map_err(|e| e.to_string())?;
    let query = serde_json::from_value(json!({"text":"atlas","interpret_locators":false}))?;
    let found = bridge.search(&query, 10, None, &Cancellation::default())?;
    assert_eq!(found.results.len(), 2);
    let mut sources: Vec<_> = found.results.iter().map(|row| row.source.as_str()).collect();
    sources.sort_unstable();
    assert_eq!(sources, ["zotero:group:7", "zotero:user"]);
    let mut status = crate::library::status::status(&crate::library::Library::default());
    bridge.status(&mut status);
    assert_eq!(status.source_counts.len(), 2);
    for row in &found.results {
        assert_eq!(status.source_counts.get(&row.source), Some(&1));
        let scoped = bridge.search(&query, 10, Some(vec![row.source.clone()]), &Cancellation::default())?;
        assert_eq!(scoped.results.len(), 1);
        assert_eq!(scoped.results[0].id, row.id);
        assert_eq!(scoped.results[0].source, row.source);
        assert_ne!(row.id["library"].as_str(), Some(row.source.as_str()));
        // Search narrowing must never bypass collisions in the loaded context.
        assert!(
            bridge
                .preview(
                    bridge.reserve_preview(),
                    vec![FileSelection {
                        id: row.id.clone(),
                        cite: row.cite.clone(),
                        locator: None,
                    }],
                    KeyFormat::Pandoc,
                    None
                )
                .is_err()
        );
    }
    assert!(
        bridge
            .search(
                &query,
                10,
                Some(vec!["zotero:group:999".into()]),
                &Cancellation::default()
            )
            .is_err()
    );
    assert!(
        bridge
            .search(&query, 10, Some(vec![]), &Cancellation::default())?
            .results
            .is_empty()
    );
    let group = ZoteroLibrary::Group(7).into();
    bridge
        .refresh_zotero(&group, false, &Cancellation::default())
        .map_err(|e| e.to_string())?;
    assert_eq!(
        bridge.search(&query, 10, None, &Cancellation::default())?.results.len(),
        1
    );
    assert!(
        bridge
            .preview(
                bridge.reserve_preview(),
                vec![selection(&bridge)?],
                KeyFormat::Pandoc,
                None
            )
            .is_ok()
    );
    let missing = bridge.refresh_zotero(&ZoteroLibrary::Group(99).into(), true, &Cancellation::default());
    assert!(missing.is_err_and(|error| error.to_string().contains("group library was not found")));
    assert_eq!(
        bridge.search(&query, 10, None, &Cancellation::default())?.results.len(),
        1
    );
    Ok(())
}

#[test]
fn full_reload_fetches_payloads_even_when_the_choice_order_differs() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_core::settings::{ZoteroLibraries, ZoteroLibrary};
    let server = fixture::Server::new();
    let engine = Engine::with_zotero_port(server.port).map_err(|_| "synthetic engine")?;
    let bridge = Arc::new(
        FileLibrary::open_zotero_with(engine, &ZoteroLibrary::Group(7).into(), &Cancellation::default())
            .map_err(|e| e.to_string())?,
    );
    let both = ZoteroLibraries::new(vec![ZoteroLibrary::Group(7), ZoteroLibrary::User]).ok_or("choice")?;
    bridge
        .refresh_zotero(&both, false, &Cancellation::default())
        .map_err(|e| e.to_string())?;
    server.pause.store(true, Ordering::SeqCst);
    let reloading = Arc::clone(&bridge);
    let worker = std::thread::spawn(move || reloading.refresh_zotero(&both, true, &Cancellation::default()));
    server.wait_for_payload();
    server.pause.store(false, Ordering::SeqCst);
    worker.join().map_err(|_| "reload worker")?.map_err(|e| e.to_string())?;
    Ok(())
}
