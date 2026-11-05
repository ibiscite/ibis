use super::*;

fn quit_with_stalled_storage(queue_full: bool) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;
    // Keep the writer alive but unable to consume work or acknowledge a save.
    let (writer, work) = mpsc::sync_channel(1);
    let history = SessionReferences {
        list: Arc::default(),
        writer,
    };
    if queue_full {
        history.writer.send(None)?;
    }
    let (returned, done) = mpsc::sync_channel(1);
    let quitting = std::thread::spawn(move || {
        returned
            .send(history.flush_timeout(Duration::from_millis(20)))
            .unwrap_or_else(|e| panic!("{e:?}"));
    });
    let pending = if queue_full {
        None
    } else {
        work.recv_timeout(Duration::from_secs(3))?
    };
    let quit = done.recv_timeout(Duration::from_secs(3));
    // Release the old implementation before asserting, so a regression cannot hang the test.
    drop(pending);
    drop(work);
    quitting.join().unwrap_or_else(|e| panic!("{e:?}"));
    assert!(
        quit.is_ok(),
        "Quit must continue while recent history storage remains stalled"
    );
    assert!(
        quit?.is_err(),
        "a timed-out Quit does not claim that recents were saved"
    );
    Ok(())
}

#[test]
fn quit_continues_with_a_full_persistence_queue() -> Result<(), Box<dyn std::error::Error>> {
    quit_with_stalled_storage(true)
}

#[test]
fn quit_continues_without_a_persistence_acknowledgement() -> Result<(), Box<dyn std::error::Error>> {
    quit_with_stalled_storage(false)
}

type SaveBarrier = (PathBuf, std::sync::mpsc::SyncSender<()>, std::sync::mpsc::Receiver<()>);
static SAVE_BARRIER: Mutex<Option<SaveBarrier>> = Mutex::new(None);
pub(super) fn before_save(path: Option<&std::path::Path>) {
    let mut barrier = SAVE_BARRIER.lock().unwrap_or_else(|e| panic!("{e:?}"));
    if barrier
        .as_ref()
        .is_some_and(|(expected, _, _)| Some(expected.as_path()) == path)
    {
        let (_, entered, release) = barrier.take().unwrap_or_else(|| panic!("a save barrier"));
        drop(barrier);
        entered.send(()).unwrap_or_else(|e| panic!("{e:?}"));
        release.recv().unwrap_or_else(|e| panic!("{e:?}"));
    }
}

#[test]
fn stalled_storage_does_not_hold_enter_and_forget_wins_after_restart() -> Result<(), Box<dyn std::error::Error>> {
    use std::{sync::mpsc, time::Duration};
    let (dir, history) = history();
    let path = saved(&history);
    let history = Arc::new(history);
    let epoch = history.lock().epoch();
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, blocked) = mpsc::sync_channel(1);
    *SAVE_BARRIER.lock().unwrap_or_else(|e| panic!("{e:?}")) = Some((path.clone(), entered, blocked));
    let (returned, copied) = mpsc::sync_channel(1);
    let worker = Arc::clone(&history);
    let old = Arc::clone(&epoch);
    let copy = std::thread::spawn(move || {
        // This is called only after the clipboard has accepted the citation.
        worker.record(&old, Some("invented.writer"), &[item("a")]);
        returned
            .send("copied; ready for delivery")
            .unwrap_or_else(|e| panic!("{e:?}"));
    });
    waiting.recv_timeout(Duration::from_secs(3))?;
    let delivered = copied.recv_timeout(Duration::from_secs(3));
    let forgetting = delivered.as_ref().ok().map(|_| {
        assert_eq!(history.lock().ids(Some("invented.writer")), [item("a")]);
        history.record(&epoch, Some("invented.writer"), &[item("newer")]);
        assert_eq!(history.lock().ids(Some("invented.writer")), [item("newer"), item("a")]);
        let history = Arc::clone(&history);
        std::thread::spawn(move || history.forget())
    });
    // Observe the visible clear while the old disk write is still stalled.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while delivered.is_ok() && history.lock().has_history() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    let cleared_while_stalled = delivered.is_ok() && !history.lock().has_history();
    if delivered.is_ok() {
        history.record(&epoch, Some("invented.writer"), &[item("delayed")]);
    }
    // Always release the worker, including on the old blocking implementation.
    release.send(())?;
    copy.join().unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(delivered?, "copied; ready for delivery", "Enter must not wait for disk");
    forgetting
        .ok_or("Forget started")?
        .join()
        .unwrap_or_else(|e| panic!("{e:?}"))?;
    assert!(
        cleared_while_stalled,
        "Forget removes visible history without waiting for disk"
    );
    history.flush()?;
    assert!(!List::open(path, true).has_history(), "Forget survives reopening");
    drop(dir);
    Ok(())
}

fn item(key: &str) -> ItemRef {
    ItemRef {
        library: ibis_core::contract::LibraryId::new("invented").unwrap_or_else(|error| panic!("{error}")),
        key: key.into(),
    }
}
fn history() -> (tempfile::TempDir, SessionReferences) {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let history = SessionReferences::default();
    *history.lock() = List::open(dir.path().join("recents.json"), true);
    (dir, history)
}
fn saved(history: &SessionReferences) -> PathBuf {
    history
        .lock()
        .path
        .clone()
        .unwrap_or_else(|| panic!("a persistent store"))
}
#[test]
fn app_isolation_restart_and_memory_only_session() {
    let (_dir, history) = history();
    let epoch = history.lock().epoch();
    history.record(&epoch, Some("invented.writer"), &[item("a"), item("b")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    history.record(&epoch, Some("invented.editor"), &[item("b")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    history.record(&epoch, None, &[item("unknown")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    history.record(&epoch, Some("invented.writer"), &[item("b"), item("b")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    let reopened = List::open(saved(&history), true);
    assert_eq!(reopened.ids(Some("invented.writer")), [item("b"), item("a")]);
    assert_eq!(reopened.ids(Some("invented.editor")), [item("b")]);
    assert!(reopened.ids(None).is_empty());
    assert!(SessionReferences::default().lock().items.is_empty());
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(saved(&history)).unwrap_or_else(|error| panic!("{error}")))
            .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        json,
        serde_json::json!([
            ["invented.writer", [item("b"), item("a")]],
            ["invented.editor", [item("b")]]
        ])
    );
}
#[test]
fn forget_and_disable_retire_delayed_writes_without_replaying_on_enable() {
    let (_dir, history) = history();
    let old = history.lock().epoch();
    history.record(&old, Some("invented.writer"), &[item("a")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    history.forget().unwrap_or_else(|error| panic!("{error}"));
    history.record(&old, Some("invented.writer"), &[item("delayed")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    assert!(!history.lock().has_history());
    let old = history.lock().epoch();
    history.set_enabled(false).unwrap_or_else(|error| panic!("{error}"));
    let current = history.lock().epoch();
    history.record(&current, Some("invented.writer"), &[item("while-off")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    assert!(!history.lock().has_history());
    history.set_enabled(true).unwrap_or_else(|error| panic!("{error}"));
    history.record(&old, Some("invented.writer"), &[item("delayed")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    assert!(!history.lock().has_history());
    let current = history.lock().epoch();
    history.record(&current, Some("invented.writer"), &[item("fresh")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(history.lock().ids(Some("invented.writer")), [item("fresh")]);
    history.set_enabled(false).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(!history.lock().has_history());
    assert!(!List::open(saved(&history), true).has_history());
}
#[test]
fn storage_failure_keeps_memory_and_failed_forget_is_retryable() {
    let (dir, history) = history();
    std::fs::create_dir(saved(&history)).unwrap_or_else(|error| panic!("{error}"));
    let epoch = history.lock().epoch();
    history.record(&epoch, Some("invented.writer"), &[item("a")]);
    assert!(history.flush().is_err());
    assert_eq!(history.lock().ids(Some("invented.writer")), [item("a")]);
    let old = history.lock().epoch();
    assert!(history.forget().is_err());
    assert!(!history.lock().has_history());
    std::fs::remove_dir(saved(&history)).unwrap_or_else(|error| panic!("{error}"));
    history.forget().unwrap_or_else(|error| panic!("{error}"));
    history.record(&old, Some("invented.writer"), &[item("delayed")]);
    history.flush().unwrap_or_else(|error| panic!("{error}"));
    assert!(!List::open(dir.path().join("recents.json"), true).has_history());
}
#[test]
fn parser_refuses_unknown_fields_controls_duplicates_and_oversized_input() {
    let (_dir, history) = history();
    let good = serde_json::json!([["invented.writer", [item("a")]]]);
    let mut bad = vec![
        serde_json::json!({"private":"invented"}),
        serde_json::json!([{"app":"invented.writer","items":[{"library":"/invented/path","key":"a"}]}]),
    ];
    for field in ["title", "query", "text", "path", "locator"] {
        let mut value = good.clone();
        value[0]
            .as_array_mut()
            .unwrap_or_else(|| panic!("an app pair"))
            .push(serde_json::json!({field: "invented-private"}));
        bad.push(value);
        let mut value = good.clone();
        value[0][1][0][field] = "invented-private".into();
        bad.push(value);
    }
    for key in ["", "a\n", "\u{0085}"] {
        let mut value = good.clone();
        value[0][1][0]["key"] = key.into();
        bad.push(value);
    }
    bad.push(serde_json::json!([good[0], good[0]]));
    bad.push(serde_json::json!([["invented.writer", [item("a"), item("a")]]]));
    bad.push(serde_json::json!([["x".repeat(513), []]]));
    for value in bad {
        std::fs::write(saved(&history), value.to_string()).unwrap_or_else(|error| panic!("{error}"));
        assert!(!List::open(saved(&history), true).has_history(), "{value}");
    }
    for raw in ["[".to_string(), " ".repeat(MAX_BYTES + 1)] {
        std::fs::write(saved(&history), raw).unwrap_or_else(|error| panic!("{error}"));
        assert!(!List::open(saved(&history), true).has_history());
    }
}
#[test]
fn bounded_history_round_trips_at_the_largest_allowed_identity() {
    let (_dir, history) = history();
    for app in 0..=20 {
        let ids: Vec<_> = (0..35)
            .map(|i| ItemRef {
                library: ibis_core::contract::LibraryId::new(&"f".repeat(64)).unwrap_or_else(|error| panic!("{error}")),
                key: format!("{i:03}{}", "x".repeat(509)),
            })
            .collect();
        let current = history.lock().epoch();
        history.record(&current, Some(&format!("invented.{app}")), &ids);
        history.flush().unwrap_or_else(|error| panic!("{error}"));
    }
    assert!(history.lock().ids(Some("invented.0")).is_empty());
    assert_eq!(history.lock().ids(Some("invented.20")).len(), 30);
    assert!(
        std::fs::metadata(saved(&history))
            .unwrap_or_else(|error| panic!("{error}"))
            .len()
            <= MAX_BYTES as u64
    );
    assert_eq!(List::open(saved(&history), true).entries, history.lock().entries);
}
#[test]
fn undo_reset_keeps_newer_citations_even_when_the_restored_list_is_full() {
    let mut list = List::default();
    list.add((0..MAX_CITATION_ITEMS).map(|n| item(&format!("old{n}"))));
    list.reset();
    list.add([item("new")]);
    list.undo();
    assert_eq!(list.items.len(), MAX_CITATION_ITEMS);
    assert!(list.items.contains(&item("new")));
    assert!(list.overflowed);
}

fn keys(list: &List) -> Vec<&str> {
    list.items.iter().map(|item| item.key.as_str()).collect()
}

#[test]
fn a_record_is_listed_once_in_the_order_first_cited() {
    let mut list = List::default();
    list.add([item("a"), item("b")]);
    list.add([item("b"), item("c"), item("a")]);
    assert_eq!(keys(&list), ["a", "b", "c"]);
}

#[test]
fn removing_can_be_undone_in_place_step_by_step() {
    let mut list = List::default();
    list.add([item("a"), item("b"), item("c")]);
    list.remove(&item("b"));
    list.remove(&item("a"));
    assert_eq!(keys(&list), ["c"]);
    list.undo();
    assert_eq!(keys(&list), ["a", "c"]);
    list.undo();
    assert_eq!(keys(&list), ["a", "b", "c"]);
    let revision = list.revision;
    list.undo();
    assert_eq!(
        (keys(&list), list.revision),
        (vec!["a", "b", "c"], revision),
        "nothing left to undo"
    );
}

#[test]
fn a_citation_copied_after_an_edit_survives_its_undo() {
    let mut list = List::default();
    list.add([item("a"), item("b")]);
    list.reset();
    assert!(list.items.is_empty());
    list.add([item("c"), item("a")]);
    list.undo();
    assert_eq!(keys(&list), ["a", "b", "c"]);

    let mut list = List::default();
    list.add([item("a")]);
    list.remove(&item("a"));
    list.add([item("a")]);
    list.undo();
    assert_eq!(keys(&list), ["a"], "never listed twice");
}

#[test]
fn resetting_an_empty_list_leaves_nothing_to_undo() {
    let mut list = List::default();
    list.reset();
    assert!(list.undo.is_empty());
}

#[test]
fn the_list_says_when_it_could_not_keep_a_citation() {
    let mut list = List::default();
    list.add((0..MAX_CITATION_ITEMS).map(|index| item(&format!("k{index}"))));
    assert!(!list.overflowed);
    list.add([item("one-more")]);
    assert!(list.overflowed);
    assert_eq!(list.items.len(), MAX_CITATION_ITEMS);
    list.reset();
    assert!(!list.overflowed);
}

#[test]
fn undoing_never_passes_the_limit_and_keeps_the_full_notice() {
    let mut list = List::default();
    list.add((0..MAX_CITATION_ITEMS).map(|index| item(&format!("k{index}"))));
    list.remove(&item("k0"));
    list.add([item("newcomer")]);
    list.undo();
    assert_eq!(list.items.len(), MAX_CITATION_ITEMS);
    assert!(list.overflowed, "k0 could not come back, and the list says so");

    let mut list = List::default();
    list.add((0..MAX_CITATION_ITEMS).map(|index| item(&format!("k{index}"))));
    list.add([item("one-more")]);
    list.reset();
    list.undo();
    assert!(list.overflowed, "the notice returns with the list");
}

#[test]
fn undo_is_bounded() {
    let mut list = List::default();
    for index in 0..=MAX_UNDO {
        let key = format!("k{index}");
        list.add([item(&key)]);
        list.remove(&item(&key));
    }
    assert_eq!(list.undo.len(), MAX_UNDO);
}

#[test]
fn recents_resolve_only_current_records_after_changes_deletion_and_source_close()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::{Cancellation, Engine, Method};
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("invented.bib");
    std::fs::write(
        &path,
        "@book{a,title={Old invented title}}\n@book{b,title={Deleted invented title}}",
    )
    .map_err(|_| "libraries")?;
    let engine = Engine::new().map_err(|_| "engine")?.without_file_watches();
    engine
        .dispatch(
            Method::Libraries,
            &serde_json::json!({"action":"open-file","path":path}),
            &Cancellation::default(),
        )
        .map_err(|_| "libraries")?;
    let snapshot = engine.snapshot().map_err(|_| "snapshot")?;
    let ids: Vec<_> = snapshot
        .objects()
        .iter()
        .map(|object| object.object.id().clone())
        .collect();
    assert_eq!(resolve(&snapshot, &ids, false).len(), 2);
    std::fs::write(&path, "@book{a,title={Current invented title}}")?;
    engine
        .dispatch(
            Method::Libraries,
            &serde_json::json!({"action":"reload"}),
            &Cancellation::default(),
        )
        .map_err(|_| "libraries")?;
    let snapshot = engine.snapshot().map_err(|_| "snapshot")?;
    let picked = resolve(&snapshot, &ids, false);
    assert_eq!(picked.len(), 1);
    assert!(
        ids.contains(picked[0].0.object.id()),
        "the surviving record keeps its identity"
    );
    assert!(matches!(&picked[0].0.object, LibraryObject::Record(record)
        if record.summary().title.as_deref() == Some("Current invented title")));
    let empty = Engine::new()
        .map_err(|_| "engine")?
        .snapshot()
        .map_err(|_| "snapshot")?;
    assert!(resolve(&empty, &ids, true).is_empty());
    Ok(())
}
