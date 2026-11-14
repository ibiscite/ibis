//! Native source selection, grouping and refresh adaptation. Citation policy is tested in ibis-engine.
use super::FileLibrary;
use crate::library::zotero_fixture as fixture;
use ibis_core::contract::DraftItem;
use ibis_engine::{Cancellation, Engine};
use serde_json::json;
use std::sync::{Arc, atomic::Ordering};

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
            crate::test_citations::preview(&bridge.engine,bridge.sample,
                    vec![DraftItem {
                prefix: None,
                suffix: None,
                        id: row.id.clone(),
                        cite: serde_json::from_value(json!(row.cite))?,
                        locator: None,
                        locator_input: None,
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
    server.instance.fetch_add(1, Ordering::Relaxed);
    let group = ZoteroLibrary::Group(7).into();
    bridge
        .refresh_zotero(&group, None, false, &Cancellation::default())
        .map_err(|e| e.to_string())?;
    assert_eq!(
        bridge.search(&query, 10, None, &Cancellation::default())?.results.len(),
        1
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
        let Loaded::Reload(reload) = loaded else {
            return Err("membership must be installed".into());
        };
        let mut library = crate::library::state::Library {
            source: Some(reload.source.clone()),
            file: Some(reload.file.clone()),
            ..Default::default()
        };
        reload.accept().map_err(|e| e.to_string())?;
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
    assert_eq!(bridge.engine.snapshot().map_err(|_| "snapshot")?.objects().len(), 1);

    // A reload that fails later keeps the same reason, so the same fix is offered.
    server.sharing_off.store(true, Ordering::Relaxed);
    let refused = bridge
        .refresh_zotero(&chosen, None, true, &Cancellation::default())
        .err()
        .ok_or("sharing went off")?;
    assert_eq!(LibraryProblem::from(refused), LibraryProblem::ZoteroAccessOff);
    Ok(())
}
