use super::*;

type SaveBarrier = (PathBuf, std::sync::mpsc::SyncSender<()>, std::sync::mpsc::Receiver<()>);
static SAVE_BARRIER: Mutex<Option<SaveBarrier>> = Mutex::new(None);
pub(super) fn before_save(path: Option<&std::path::Path>) {
    let mut barrier = SAVE_BARRIER.recover_lock();
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
    *SAVE_BARRIER.recover_lock() = Some((path.clone(), entered, blocked));
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
#[test]
fn each_apps_reference_list_holds_this_runs_citations_there_and_follows_the_list() {
    let (_dir, history) = history();
    let epoch = history.lock().epoch();
    let cite = |app: &str, items: &[ItemRef]| {
        history.lock().add(items.iter().cloned());
        history.record(&epoch, Some(app), items);
    };
    cite("invented.writer", &[item("a"), item("b")]);
    cite("invented.editor", &[item("c")]);
    cite("invented.writer", &[item("b"), item("c")]);
    assert_eq!(history.lock().cited_here("invented.writer"), [item("a"), item("b"), item("c")]);
    assert_eq!(history.lock().cited_here("invented.editor"), [item("c")]);
    assert!(history.lock().cited_here("invented.other").is_empty());
    // Remove, Reset List and its Undo apply to every app's reference list.
    history.lock().remove(&item("b"));
    assert_eq!(history.lock().cited_here("invented.writer"), [item("a"), item("c")]);
    history.lock().reset();
    assert!(history.lock().cited_here("invented.writer").is_empty());
    history.lock().undo();
    assert_eq!(history.lock().cited_here("invented.writer"), [item("a"), item("c")]);
    // Off the disk: a new run starts with none, and Forget Recent Citations clears them.
    assert!(List::open(saved(&history), true).cited_here("invented.writer").is_empty());
    history.forget().unwrap_or_else(|error| panic!("{error}"));
    assert!(history.lock().cited_here("invented.writer").is_empty());
}
