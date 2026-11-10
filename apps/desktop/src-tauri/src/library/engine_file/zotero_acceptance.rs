//! The real native bridge over an invented raw Zotero endpoint, never discovery.
#[path = "../../../../../../crates/ibis-engine/tests/support/zotero.rs"]
mod fixture;
use super::{FileLibrary, FileSelection};
use ibis_core::contract::CitationResult;
use ibis_engine::{Cancellation, Engine, Method};
use serde_json::json;
use std::{
    io::Cursor,
    sync::{Arc, atomic::Ordering},
};

fn opened(server: &fixture::Server) -> Result<FileLibrary, Box<dyn std::error::Error>> {
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
    for (style, option, value) in [(None, "--format", "pandoc"), (Some("apa"), "--style", "apa")] {
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
                style.unwrap_or("pandoc"),
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
        .preview(bridge.reserve_preview(), vec![selection(&bridge)?], "pandoc")
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
        .preview(bridge.reserve_preview(), vec![selection(&bridge)?], "pandoc")
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
    let bridge = std::sync::Arc::new(
        FileLibrary::open_zotero_with(engine, &both, None, &Cancellation::default()).map_err(|e| e.to_string())?,
    );
    let query = serde_json::from_value(json!({"text":"atlas","interpret_locators":false}))?;
    let found = bridge.search(&query, 10, None, &Cancellation::default())?;
    assert_eq!(found.results.len(), 2);
    let mut sources: Vec<_> = found.results.iter().map(|row| row.source.as_str()).collect();
    sources.sort_unstable();
    assert_eq!(sources, ["zotero:group:7", "zotero:user"]);
    let status = crate::library::status::status(&crate::library::Library {
        source: Some(crate::library::Source::Zotero(both.clone())),
        file: Some(bridge.clone()),
        ..Default::default()
    });
    assert_eq!(status.source_counts.len(), 2);
    for row in &found.results {
        assert_eq!(status.source_counts.get(&row.source), Some(&1));
        let scoped = bridge.search(&query, 10, Some(vec![row.source.clone()]), &Cancellation::default())?;
        assert_eq!(scoped.results.len(), 1);
        assert_eq!(scoped.results[0].id, row.id);
        assert_eq!(scoped.results[0].source, row.source);
        assert_ne!(row.id.library.as_str(), row.source.as_str());
        // Search narrowing must never bypass collisions in the loaded context.
        assert!(matches!(
            bridge
                .preview(
                    bridge.reserve_preview(),
                    vec![FileSelection {
                prefix: None,
                suffix: None,
                        id: row.id.clone(),
                        cite: serde_json::from_value(json!(row.cite))?,
                        locator: None,
                        page_input: None,
                        form: None,
                    }],
                    "pandoc"
                ),
            Err(crate::library::CommandError::SharedCitationKey { key }) if key == "Synthetic2026"
        ));
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
        .refresh_zotero(&group, None, false, &Cancellation::default())
        .map_err(|e| e.to_string())?;
    assert_eq!(
        bridge.search(&query, 10, None, &Cancellation::default())?.results.len(),
        1
    );
    assert!(
        bridge
            .preview(bridge.reserve_preview(), vec![selection(&bridge)?], "pandoc")
            .is_ok()
    );
    let missing = bridge.refresh_zotero(&ZoteroLibrary::Group(99).into(), None, true, &Cancellation::default());
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
        FileLibrary::open_zotero_with(engine, &ZoteroLibrary::Group(7).into(), None, &Cancellation::default())
            .map_err(|e| e.to_string())?,
    );
    let both = ZoteroLibraries::new(vec![ZoteroLibrary::Group(7), ZoteroLibrary::User]).ok_or("choice")?;
    bridge
        .refresh_zotero(&both, None, false, &Cancellation::default())
        .map_err(|e| e.to_string())?;
    // Deliberately choose the opposite of the engine's ID-sorted listing.
    let reversed = bridge
        .engine
        .libraries()
        .map_err(|_| "listing")?
        .libraries
        .iter()
        .rev()
        .filter_map(|info| {
            info.zotero.map(|library| match library {
                ibis_engine::request::ZoteroLibrary::Personal => ZoteroLibrary::User,
                ibis_engine::request::ZoteroLibrary::Group(id) => ZoteroLibrary::Group(id),
            })
        })
        .collect();
    let both = ZoteroLibraries::new(reversed).ok_or("reversed choice")?;
    bridge
        .refresh_zotero(&both, None, false, &Cancellation::default())
        .map_err(|e| e.to_string())?;
    server.pause.store(true, Ordering::SeqCst);
    let reloading = Arc::clone(&bridge);
    let worker = std::thread::spawn(move || reloading.refresh_zotero(&both, None, true, &Cancellation::default()));
    server.wait_for_payload();
    server.pause.store(false, Ordering::SeqCst);
    worker.join().map_err(|_| "reload worker")?.map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn empty_group_membership_is_installed_even_when_records_are_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    use crate::library::{
        Source,
        load::{Loaded, load_source},
    };
    use ibis_core::settings::{ZoteroLibraries, ZoteroLibrary};
    let server = fixture::Server::new();
    server.empty_group.store(true, Ordering::SeqCst);
    let engine = Engine::with_zotero_port(server.port).map_err(|_| "synthetic engine")?;
    let mut bridge = Arc::new(
        FileLibrary::open_zotero_with(engine, &ZoteroLibrary::User.into(), None, &Cancellation::default())
            .map_err(|e| e.to_string())?,
    );
    let revision = bridge.revision();
    let both = ZoteroLibraries::new(vec![ZoteroLibrary::User, ZoteroLibrary::Group(7)]).ok_or("choice")?;
    for chosen in [both, ZoteroLibrary::User.into()] {
        let source = Source::Zotero(chosen.clone());
        let loaded = load_source(
            source.clone(),
            false,
            Some(bridge),
            None,
            &Cancellation::default(),
            || false,
        )
        .map_err(|e| e.to_string())?;
        let Loaded::New(mut library) = loaded else {
            return Err("membership must be installed".into());
        };
        assert_eq!(library.source, Some(source));
        assert_eq!(
            library.source.as_ref().ok_or("source")?.setting(),
            Some(ibis_core::settings::LibrarySetting::Zotero(chosen.clone()))
        );
        let status = crate::library::status::status(&library);
        assert_eq!(status.zotero_libraries, chosen.libraries());
        assert_eq!(status.references, 1);
        assert_eq!(
            status.source_counts.get("zotero:group:7"),
            chosen.libraries().contains(&ZoteroLibrary::Group(7)).then_some(&0)
        );
        bridge = library.file.take().ok_or("bridge")?;
        assert_eq!(bridge.revision(), revision);
    }
    Ok(())
}

#[test]
fn zotero_that_turns_sharing_on_opens_with_no_change_to_the_choice() -> Result<(), Box<dyn std::error::Error>> {
    use crate::library::LoadError;
    use crate::library::status::LibraryProblem;
    use ibis_core::settings::{ZoteroLibraries, ZoteroLibrary};
    let server = fixture::Server::new();
    server.sharing_off.store(true, Ordering::Relaxed);
    let chosen: ZoteroLibraries = ZoteroLibrary::User.into();
    let open = || {
        let engine = Engine::with_zotero_port(server.port).map_err(LoadError::Zotero)?;
        FileLibrary::open_zotero_with(engine, &chosen, None, &Cancellation::default())
    };
    let refused = open().err().ok_or("sharing is off")?;
    assert_eq!(LibraryProblem::from(refused), LibraryProblem::ZoteroAccessOff);
    server.sharing_off.store(false, Ordering::Relaxed);
    let bridge = open().map_err(|e| e.to_string())?;
    selection(&bridge)?;

    // A reload that fails later keeps the same reason, so the same fix is offered.
    server.sharing_off.store(true, Ordering::Relaxed);
    let refused = bridge
        .refresh_zotero(&chosen, None, true, &Cancellation::default())
        .err()
        .ok_or("sharing went off")?;
    assert_eq!(LibraryProblem::from(refused), LibraryProblem::ZoteroAccessOff);
    Ok(())
}

#[test]
fn a_copy_waiting_for_its_commit_is_refused_by_a_newer_preview_or_a_zotero_read()
-> Result<(), Box<dyn std::error::Error>> {
    let server = fixture::Server::new();
    let bridge = Arc::new(opened(&server)?);
    let shown = bridge
        .preview(bridge.reserve_preview(), vec![selection(&bridge)?], "pandoc")
        .map_err(|e| e.to_string())?;
    // A preview asked for after the copy was re-cited, even one that then fails.
    let recited = bridge
        .recite(&shown.ticket, &shown.content_hash)
        .map_err(|e| e.to_string())?;
    let _newer = bridge.reserve_preview();
    assert!(
        bridge
            .copy_recited(&recited, |_| panic!("a superseded copy must not write"))
            .is_err()
    );

    // A full Zotero read that begins between re-citing and writing.
    let shown = bridge
        .preview(bridge.reserve_preview(), vec![selection(&bridge)?], "pandoc")
        .map_err(|e| e.to_string())?;
    let recited = bridge
        .recite(&shown.ticket, &shown.content_hash)
        .map_err(|e| e.to_string())?;
    server.instance.store(2, Ordering::SeqCst);
    server.pause.store(true, Ordering::SeqCst);
    let reloading = Arc::clone(&bridge);
    let worker = std::thread::spawn(move || reloading.reload());
    server.wait_for_payload();
    let refused = bridge.copy_recited(&recited, |_| panic!("an uncertain Zotero must not write"));
    server.pause.store(false, Ordering::SeqCst);
    worker
        .join()
        .map_err(|_| "reload worker")?
        .map_err(|_| "coherent reload")?;
    assert!(refused.is_err());
    Ok(())
}
