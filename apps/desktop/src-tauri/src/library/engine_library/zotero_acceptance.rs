//! Native source selection, grouping and refresh adaptation. Citation policy is tested in ibis-engine.
use super::FileLibrary;
use crate::library::tests::open;
use crate::library::zotero_fixture as fixture;
use ibis_core::contract::DraftItem;
use ibis_engine::library::{Intent, Owner, Source};
use ibis_engine::{Cancellation, Engine};
use serde_json::json;
use std::sync::{Arc, atomic::Ordering};

fn owner(directory: &std::path::Path, port: u16) -> Owner {
    Owner::new(
        Arc::new(crate::preferences::SettingsFile::new(directory.join("settings.json"))),
        move || Engine::with_zotero_port(port),
        |_| {},
    )
}

#[test]
fn chosen_groups_keep_source_identity_counts_and_scopes_consistent() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_engine::settings::{ZoteroLibraries, ZoteroLibrary};
    let server = fixture::Server::new();
    let directory = tempfile::tempdir()?;
    let owner = owner(directory.path(), server.port);
    let both = ZoteroLibraries::new(vec![ZoteroLibrary::Group(7), ZoteroLibrary::User]).ok_or("choice")?;
    open(&owner, Source::Zotero(both.clone()));
    let bridge = owner.current().ok_or("current")?;
    let query = serde_json::from_value(json!({"text":"atlas","interpret_locators":false}))?;
    let found = bridge.search(&query, 10, None, &Cancellation::default())?;
    assert_eq!(found.results.len(), 2);
    let mut sources: Vec<_> = found.results.iter().map(|row| row.source.as_str()).collect();
    sources.sort_unstable();
    assert_eq!(sources, ["zotero:group:7", "zotero:user"]);
    let status = owner.status();
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
            crate::test_citations::preview(&bridge.engine,bridge.sample(),
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
            Err(error) if error.problem().code == "ambiguous-key" && error.problem().info.message.args.get("key").is_some_and(|key| key == "Synthetic2026")
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
    open(&owner, Source::Zotero(group));
    let bridge = owner.current().ok_or("current")?;
    assert_eq!(
        bridge.search(&query, 10, None, &Cancellation::default())?.results.len(),
        1
    );
    open(&owner, Source::Zotero(ZoteroLibrary::Group(99).into()));
    assert!(
        owner
            .status()
            .error
            .is_some_and(|error| error.contains("group library was not found"))
    );
    assert_eq!(
        bridge.search(&query, 10, None, &Cancellation::default())?.results.len(),
        1
    );
    Ok(())
}

#[test]
fn full_reload_fetches_payloads_even_when_the_choice_order_differs() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_engine::settings::{ZoteroLibraries, ZoteroLibrary};
    let server = fixture::Server::new();
    let directory = tempfile::tempdir()?;
    let owner = owner(directory.path(), server.port);
    open(&owner, Source::Zotero(ZoteroLibrary::Group(7).into()));
    let both = ZoteroLibraries::new(vec![ZoteroLibrary::Group(7), ZoteroLibrary::User]).ok_or("choice")?;
    open(&owner, Source::Zotero(both));
    let bridge = owner.current().ok_or("current")?;
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
    open(&owner, Source::Zotero(both));
    server.pause.store(true, Ordering::SeqCst);
    owner.submit(Intent::Reload);
    server.wait_for_payload();
    server.pause.store(false, Ordering::SeqCst);
    assert!(owner.wait_idle(std::time::Duration::from_secs(10)));
    assert!(owner.status().error.is_none());
    Ok(())
}

#[test]
fn empty_group_membership_is_installed_even_when_records_are_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_engine::settings::{ZoteroLibraries, ZoteroLibrary};
    let server = fixture::Server::new();
    server.empty_group.store(true, Ordering::SeqCst);
    let directory = tempfile::tempdir()?;
    let owner = owner(directory.path(), server.port);
    open(&owner, Source::Zotero(ZoteroLibrary::User.into()));
    let revision = owner.current().ok_or("current")?.revision();
    let both = ZoteroLibraries::new(vec![ZoteroLibrary::User, ZoteroLibrary::Group(7)]).ok_or("choice")?;
    for chosen in [both, ZoteroLibrary::User.into()] {
        open(&owner, Source::Zotero(chosen.clone()));
        let status = owner.status();
        assert_eq!(status.zotero_libraries, chosen.libraries());
        assert_eq!(status.references, 1);
        assert_eq!(
            status.source_counts.get("zotero:group:7"),
            chosen.libraries().contains(&ZoteroLibrary::Group(7)).then_some(&0)
        );
        assert_eq!(owner.current().ok_or("current")?.revision(), revision);
    }
    Ok(())
}

#[test]
fn zotero_that_turns_sharing_on_opens_with_no_change_to_the_choice() -> Result<(), Box<dyn std::error::Error>> {
    use ibis_engine::library::LibraryProblem;
    use ibis_engine::settings::ZoteroLibrary;
    let server = fixture::Server::new();
    server.sharing_off.store(true, Ordering::Relaxed);
    let directory = tempfile::tempdir()?;
    let owner = owner(directory.path(), server.port);
    open(&owner, Source::Zotero(ZoteroLibrary::User.into()));
    assert_eq!(owner.status().problem, Some(LibraryProblem::ZoteroAccessOff));
    server.sharing_off.store(false, Ordering::Relaxed);
    owner.submit(Intent::RetryFailed);
    assert!(owner.wait_idle(std::time::Duration::from_secs(10)));
    assert_eq!(
        owner
            .current()
            .ok_or("current")?
            .engine
            .snapshot()
            .map_err(|_| "snapshot")?
            .objects()
            .len(),
        1
    );
    server.sharing_off.store(true, Ordering::Relaxed);
    owner.submit(Intent::Reload);
    assert!(owner.wait_idle(std::time::Duration::from_secs(10)));
    assert_eq!(owner.status().problem, Some(LibraryProblem::ZoteroAccessOff));
    Ok(())
}
