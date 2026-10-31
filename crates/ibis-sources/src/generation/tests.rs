use super::*;
use ibis_core::contract::{Record, Snapshot};
use serde_json::json;

fn library() -> LibraryId {
    LibraryId::file(b"invented-generation-source")
}

fn item(key: &str, title: &str) -> LibraryObject {
    LibraryObject::Record(Record {
        id: ItemRef { library: library(), key: key.into() },
        csl: serde_json::from_value(json!({"type":"book","title":title,"author":[{"family":"Invented"}],"issued":{"date-parts":[[2026]]},"abstract":"Lazy invented detail"}))
            .unwrap_or_else(|e| panic!("{e}")),
        citation_keys: vec![],
        aliases: vec![],
        warnings: vec![],
        identifiers: vec![],
    })
}

fn open() -> (tempfile::TempDir, GenerationStore) {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let store = GenerationStore::open(&directory.path().join("generations")).unwrap_or_else(|e| panic!("{e}"));
    (directory, store)
}

fn set_limits(store: &mut GenerationStore, files: usize, bytes: u64) {
    Arc::get_mut(&mut store.inner)
        .unwrap_or_else(|| panic!("unexpected authority owner"))
        .limits = StoreLimits { files, bytes };
}

fn disk_usage(store: &GenerationStore) -> (usize, u64) {
    std::fs::read_dir(store.inner.directory.path())
        .unwrap_or_else(|e| panic!("{e}"))
        .map(|entry| {
            entry
                .and_then(|entry| entry.metadata())
                .unwrap_or_else(|e| panic!("{e}"))
                .len()
        })
        .fold((0, 0), |(files, bytes), len| (files + 1, bytes + len))
}

fn in_library(mut object: LibraryObject, library: &LibraryId) -> LibraryObject {
    let LibraryObject::Record(record) = &mut object else {
        panic!("wrong fixture kind")
    };
    record.id.library = library.clone();
    object
}

fn abandoned_file(store: &GenerationStore, prefix: &str, size: u64) -> PathBuf {
    let mut file = tempfile::Builder::new()
        .prefix(prefix)
        .tempfile_in(store.inner.directory.path())
        .unwrap_or_else(|e| panic!("{e}"));
    file.write_all(MAGIC).unwrap_or_else(|e| panic!("{e}"));
    file.as_file().set_len(size).unwrap_or_else(|e| panic!("{e}"));
    let (file, path) = file.keep().unwrap_or_else(|e| panic!("{e}"));
    drop(file);
    path
}

#[test]
fn abandoned_crash_stages_are_reclaimed_after_restart() {
    let (directory, store) = open();
    let writers: Vec<_> = (0..5)
        .map(|source| {
            store
                .begin(LibraryId::file(&[source]), Fingerprint::of_bytes(b"crashed import"))
                .unwrap_or_else(|e| panic!("{e}"))
        })
        .collect();
    let mut abandoned = vec![];
    for mut writer in writers {
        writer
            .stage
            .file()
            .unwrap_or_else(|e| panic!("{e}"))
            .as_file()
            .set_len(4096)
            .unwrap_or_else(|e| panic!("{e}"));
        let (file, path) = writer
            .stage
            .file
            .take()
            .unwrap_or_else(|| panic!("missing stage"))
            .keep()
            .unwrap_or_else(|e| panic!("{e}"));
        drop(file); // Simulate process death: no temporary-file destructor runs.
        abandoned.push(path);
    }
    drop(store);
    let mut reopened = GenerationStore::open(&directory.path().join("generations")).unwrap_or_else(|e| panic!("{e}"));
    set_limits(&mut reopened, 16, 16 * 1024);
    let accepted = reopened
        .publish(stage(&reopened, "Recovered after crash"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(title(&get(&accepted, "record:one")), "Recovered after crash");
    assert!(abandoned.iter().all(|path| !path.exists()));
}

#[test]
fn concurrent_payload_writers_reserve_the_aggregate_byte_budget() {
    let (_directory, mut store) = open();
    let mut first = item("record:one", "Accepted");
    if let LibraryObject::Record(record) = &mut first {
        record.csl.insert("abstract".into(), json!("x".repeat(4096)));
    }
    let mut second = first.clone();
    let second_library = LibraryId::file(b"another invented source");
    if let LibraryObject::Record(record) = &mut second {
        record.id.library = second_library.clone();
    }
    let payload = encoded(
        &SnapshotObject::new(first.clone()).unwrap_or_else(|e| panic!("{e}")),
        MAX_OBJECT,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .len() as u64;
    let limit = disk_usage(&store).1 + 2 * MAGIC.len() as u64 + 2 * payload - 1;
    set_limits(&mut store, 16, limit);
    let mut a = store
        .begin(library(), Fingerprint::of_bytes(b"first"))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut b = store
        .begin(second_library.clone(), Fingerprint::of_bytes(b"second"))
        .unwrap_or_else(|e| panic!("{e}"));
    a.push(first).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.push(second), Err(StoreError::Limit));
    assert!(disk_usage(&store).1 <= limit);
    assert!(
        store
            .open_library(&second_library)
            .unwrap_or_else(|e| panic!("{e}"))
            .is_none()
    );
    let accepted = store
        .publish(a.finish().unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(title(&get(&accepted, "record:one")), "Accepted");
}

#[test]
fn an_oversized_directory_recovers_when_generation_readers_release() {
    for restart in [false, true] {
        let (directory, mut store) = open();
        let mut readers = vec![];
        for _ in 0..5 {
            readers.push(
                store
                    .publish(stage(&store, "Retained"))
                    .unwrap_or_else(|e| panic!("{e}")),
            );
        }
        assert_eq!(disk_usage(&store).0, 9);
        // Reproduce the on-disk state left by the old eight-file admission check.
        set_limits(&mut store, 8, 64 * 1024);
        assert!(matches!(
            store.begin(library(), Fingerprint::of_bytes(b"readers still held")),
            Err(StoreError::Limit)
        ));
        assert!(readers.iter().all(|reader| reader.path.exists()));
        drop(readers);
        if restart {
            drop(store);
            store = GenerationStore::open(&directory.path().join("generations")).unwrap_or_else(|e| panic!("{e}"));
            set_limits(&mut store, 8, 64 * 1024);
        }
        let accepted = store
            .publish(stage(&store, "Recovered at file limit"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(title(&get(&accepted, "record:one")), "Recovered at file limit");
        assert!(disk_usage(&store).0 <= 8);
    }
}

#[test]
fn concurrent_indexes_reserve_the_aggregate_byte_budget() {
    let (_directory, mut store) = open();
    let text = "x".repeat(1024);
    let probe = stage(&store, &text);
    let limit =
        disk_usage(&store).1 - probe.index_start - probe.index_len + 2 * (probe.index_start + probe.index_len) - 1;
    drop(probe);
    set_limits(&mut store, 16, limit);
    let other = LibraryId::file(b"another invented source");
    let mut a = store
        .begin(library(), Fingerprint::of_bytes(b"a"))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut b = store
        .begin(other.clone(), Fingerprint::of_bytes(b"b"))
        .unwrap_or_else(|e| panic!("{e}"));
    a.push(item("record:one", &text)).unwrap_or_else(|e| panic!("{e}"));
    b.push(in_library(item("record:one", &text), &other))
        .unwrap_or_else(|e| panic!("{e}"));
    let a = a.finish().unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(b.finish(), Err(StoreError::Limit)));
    assert!(disk_usage(&store).1 <= limit);
    assert!(store.open_library(&other).unwrap_or_else(|e| panic!("{e}")).is_none());
    let accepted = store.publish(a).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(title(&get(&accepted, "record:one")), text);
}

#[test]
fn manifest_bytes_are_reserved_before_replacing_the_accepted_generation() {
    let (_directory, mut store) = open();
    let old = store
        .publish(stage(&store, "Accepted"))
        .unwrap_or_else(|e| panic!("{e}"));
    let manifest = store.inner.directory.path().join(manifest_name(&library()));
    let previous = std::fs::read(&manifest).unwrap_or_else(|e| panic!("{e}"));
    let probe = stage(&store, "Candidate");
    let limit = disk_usage(&store).1;
    drop(probe);
    set_limits(&mut store, 16, limit);
    let candidate = stage(&store, "Candidate");
    assert_eq!(disk_usage(&store).1, limit);
    assert!(matches!(store.publish(candidate), Err(StoreError::Limit)));
    assert_eq!(std::fs::read(&manifest).unwrap_or_else(|e| panic!("{e}")), previous);
    assert_eq!(title(&get(&old, "record:one")), "Accepted");
    let reopened = store
        .open_library(&library())
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("cache miss"));
    assert_eq!(get(&old, "record:one"), get(&reopened, "record:one"));
    assert!(
        store
            .inner
            .state
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .stages
            .is_empty()
    );
}

#[test]
fn file_slots_are_reserved_for_stages_and_publication_scratch() {
    let (_directory, mut store) = open();
    let base = disk_usage(&store).0;
    let limit = base + 3;
    set_limits(&mut store, limit, 64 * 1024);
    let a = stage(&store, "Retired A");
    let b = stage(&store, "Accepted B");
    let other = LibraryId::file(b"another invented source");
    let c = store
        .begin(other, Fingerprint::of_bytes(b"c"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(disk_usage(&store).0, limit);
    assert!(matches!(
        store.begin(library(), Fingerprint::of_bytes(b"full")),
        Err(StoreError::Limit)
    ));
    assert_eq!(disk_usage(&store).0, limit);
    // B is current, but cannot create its publication scratch at the cap.
    assert!(matches!(store.publish(b), Err(StoreError::Limit)));
    assert!(a.stage.file().unwrap_or_else(|e| panic!("{e}")).path().exists());
    assert!(c.stage.file().unwrap_or_else(|e| panic!("{e}")).path().exists());
    drop(a);
    drop(c);
    let accepted = store
        .publish(stage(&store, "Recovered B"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(title(&get(&accepted, "record:one")), "Recovered B");
    assert!(disk_usage(&store).0 <= limit);
}

#[test]
fn payload_and_index_io_failures_release_reservations_while_failed_handles_survive() {
    for fail_index in [false, true] {
        let (_directory, mut store) = open();
        set_limits(&mut store, 8, 16 * 1024);
        let accepted = store
            .publish(stage(&store, "Accepted"))
            .unwrap_or_else(|e| panic!("{e}"));
        let baseline = disk_usage(&store);
        let mut writer = store
            .begin(library(), Fingerprint::of_bytes(b"failed I/O"))
            .unwrap_or_else(|e| panic!("{e}"));
        if fail_index {
            writer
                .push(item("record:one", "Failed index"))
                .unwrap_or_else(|e| panic!("{e}"));
        }
        // Fail after reserving the complete write and writing only its prefix.
        store.inner.fail_write.store(true, std::sync::atomic::Ordering::Relaxed);
        let failed_writer = if fail_index {
            assert!(matches!(writer.finish(), Err(StoreError::Unavailable)));
            None
        } else {
            assert_eq!(
                writer.push(item("record:one", "Failed payload")),
                Err(StoreError::Unavailable)
            );
            assert_eq!(disk_usage(&store), baseline);
            Some(writer)
        };
        store
            .inner
            .fail_write
            .store(false, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(disk_usage(&store), baseline);
        assert!(
            store
                .inner
                .state
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .stages
                .is_empty()
        );
        let current = store
            .publish(stage(&store, "After I/O failure"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(title(&get(&current, "record:one")), "After I/O failure");
        assert_eq!(title(&get(&accepted, "record:one")), "Accepted");
        if let Some(writer) = failed_writer {
            assert!(matches!(writer.finish(), Err(StoreError::Invalid)));
        }
    }
}

#[test]
fn failed_reserved_header_and_dropped_stages_release_file_slots() {
    let (_directory, mut store) = open();
    let base = disk_usage(&store);
    set_limits(&mut store, base.0 + 2, 4096);
    store.inner.fail_write.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(
        store.begin(library(), Fingerprint::of_bytes(b"failed header")),
        Err(StoreError::Unavailable)
    ));
    assert_eq!(disk_usage(&store), base);
    store
        .inner
        .fail_write
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let writer = store
        .begin(library(), Fingerprint::of_bytes(b"cancelled writer"))
        .unwrap_or_else(|e| panic!("{e}"));
    drop(writer);
    assert_eq!(disk_usage(&store), base);
    let staged = stage(&store, "Cancelled stage");
    drop(staged);
    assert_eq!(disk_usage(&store), base);
    let accepted = store
        .publish(stage(&store, "Accepted"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(title(&get(&accepted, "record:one")), "Accepted");
}

#[test]
fn failed_reclamation_is_a_cache_miss_and_recovers_on_retry() {
    let (_directory, mut store) = open();
    set_limits(&mut store, 8, 4096);
    let abandoned = abandoned_file(&store, "stage-", 8192);
    store
        .inner
        .fail_reclaim
        .store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(
        store.begin(library(), Fingerprint::of_bytes(b"refused")),
        Err(StoreError::Unavailable)
    ));
    assert!(abandoned.exists());
    assert!(
        store
            .open_library(&library())
            .unwrap_or_else(|e| panic!("{e}"))
            .is_none()
    );
    assert!(
        store
            .inner
            .state
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .stages
            .is_empty()
    );
    store
        .inner
        .fail_reclaim
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let accepted = store
        .publish(stage(&store, "Recovered"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(!abandoned.exists());
    assert_eq!(title(&get(&accepted, "record:one")), "Recovered");
}

#[test]
fn stale_and_closed_handles_cannot_reserve_or_collect_other_files() {
    for closed in [false, true] {
        let (_directory, store) = open();
        let mut a = store
            .begin(library(), Fingerprint::of_bytes(b"late A"))
            .unwrap_or_else(|e| panic!("{e}"));
        a.push(item("record:one", "Late A")).unwrap_or_else(|e| panic!("{e}"));
        let old_path = a.stage.file().unwrap_or_else(|e| panic!("{e}")).path().to_path_buf();
        let b = store
            .publish(stage(&store, "Accepted B"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(old_path.exists(), "a superseded live writer still owns its stage");
        let orphan = abandoned_file(&store, "generation-", MAGIC.len() as u64);
        let other = LibraryId::file(b"another invented source");
        let live = store
            .begin(other, Fingerprint::of_bytes(b"still live"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(!orphan.exists());
        let orphan = abandoned_file(&store, "generation-", MAGIC.len() as u64);
        let live_path = live.stage.file().unwrap_or_else(|e| panic!("{e}")).path().to_path_buf();
        if closed {
            store.close().unwrap_or_else(|e| panic!("{e}"));
        }
        let expected = if closed { StoreError::Closed } else { StoreError::Stale };
        assert!(matches!(a.finish(), Err(error) if error == expected));
        assert!(orphan.exists(), "retired operations cannot trigger reclamation");
        assert!(live_path.exists());
        assert!(!old_path.exists(), "only the retired handle's own stage is released");
        assert_eq!(title(&get(&b, "record:one")), "Accepted B");
    }
}

#[test]
fn hostile_directory_beyond_the_scan_bound_does_not_delete_generations() {
    let (_directory, mut store) = open();
    set_limits(&mut store, 8, 64 * 1024);
    let accepted = store
        .publish(stage(&store, "Accepted"))
        .unwrap_or_else(|e| panic!("{e}"));
    let orphans: Vec<_> = (0..32)
        .map(|_| abandoned_file(&store, "generation-", MAGIC.len() as u64))
        .collect();
    assert!(matches!(
        store.begin(library(), Fingerprint::of_bytes(b"hostile")),
        Err(StoreError::Limit)
    ));
    assert!(orphans.iter().all(|path| path.exists()));
    assert!(accepted.path.exists());
    assert_eq!(title(&get(&accepted, "record:one")), "Accepted");
}

#[test]
fn partial_headers_and_manifest_stages_recover_but_unowned_bytes_are_preserved() {
    let (_directory, store) = open();
    let stages: Vec<_> = [0, 3, MAGIC.len() as u64]
        .into_iter()
        .map(|size| abandoned_file(&store, "stage-", size))
        .collect();
    let manifest_stage = abandoned_file(&store, "manifest-stage-", 0);
    let foreign = abandoned_file(&store, "stage-", MAGIC.len() as u64);
    std::fs::write(&foreign, b"NOTIBIS!").unwrap_or_else(|e| panic!("{e}"));
    let accepted = store
        .publish(stage(&store, "Recovered"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(stages.iter().all(|path| !path.exists()));
    assert!(!manifest_stage.exists());
    assert!(foreign.exists());
    assert_eq!(title(&get(&accepted, "record:one")), "Recovered");
}

fn stage(store: &GenerationStore, title: &str) -> StagedGeneration {
    let mut writer = store
        .begin(library(), Fingerprint::of_bytes(title.as_bytes()))
        .unwrap_or_else(|e| panic!("{e}"));
    writer.push(item("record:one", title)).unwrap_or_else(|e| panic!("{e}"));
    writer.finish().unwrap_or_else(|e| panic!("{e}"))
}

fn get(generation: &PreparedLibrary, key: &str) -> SnapshotObject {
    generation
        .get(&ItemRef {
            library: library(),
            key: key.into(),
        })
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("missing invented object"))
}

fn title(object: &SnapshotObject) -> &str {
    let LibraryObject::Record(record) = &object.object else {
        panic!("wrong object kind")
    };
    record.csl["title"].as_str().unwrap_or_else(|| panic!("missing title"))
}

#[test]
fn streamed_objects_survive_input_drop_reopen_and_selected_reads() {
    let (directory, store) = open();
    let mut originals = [item("record:z", "Last"), item("record:a", "First")];
    for object in &mut originals {
        if let LibraryObject::Record(record) = object {
            record.aliases.push("shared-key".into());
        }
    }
    let expected = Snapshot::new(vec![originals[0].clone()])
        .unwrap_or_else(|e| panic!("{e}"))
        .objects()[0]
        .as_ref()
        .clone();
    let mut writer = store
        .begin(library(), Fingerprint::of_bytes(b"observed source"))
        .unwrap_or_else(|e| panic!("{e}"));
    for object in originals {
        writer.push(object).unwrap_or_else(|e| panic!("{e}"));
    }
    let generation = store
        .publish(writer.finish().unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        generation.summaries().map(|row| row.title).collect::<Vec<_>>(),
        ["First", "Last"]
    );
    assert_eq!(get(&generation, "record:z"), expected);
    drop(generation);
    drop(store);
    let reopened = GenerationStore::open(&directory.path().join("generations")).unwrap_or_else(|e| panic!("{e}"));
    let generation = reopened
        .open_library(&library())
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("cache miss"));
    assert_eq!(get(&generation, "record:z"), expected);
    assert_eq!(generation.summaries().count(), 2);
    assert_eq!(generation.object_fingerprints().count(), 2);
    assert_eq!(
        generation.key_owners().map(|(key, _)| key).collect::<Vec<_>>(),
        ["shared-key", "shared-key"]
    );
    assert!(
        generation
            .get(&ItemRef {
                library: library(),
                key: "absent".into()
            })
            .unwrap_or_else(|e| panic!("{e}"))
            .is_none()
    );
}

#[test]
fn delayed_a_cannot_replace_b_and_old_readers_keep_their_generation() {
    let (_directory, store) = open();
    let old = store
        .publish(stage(&store, "Original"))
        .unwrap_or_else(|e| panic!("{e}"));
    let a = stage(&store, "Delayed A");
    let b = stage(&store, "Accepted B");
    let current = store.publish(b).unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(store.publish(a), Err(StoreError::Stale)));
    assert_eq!(title(&get(&old, "record:one")), "Original");
    assert_eq!(title(&get(&current, "record:one")), "Accepted B");
    let reopened = store
        .open_library(&library())
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("cache miss"));
    assert_eq!(title(&get(&reopened, "record:one")), "Accepted B");
}

#[test]
fn failed_manifest_publication_retains_the_accepted_generation() {
    let (_directory, mut store) = open();
    set_limits(&mut store, 8, 16 * 1024);
    let old = store
        .publish(stage(&store, "Accepted"))
        .unwrap_or_else(|e| panic!("{e}"));
    let candidate = stage(&store, "Must not publish");
    store
        .inner
        .fail_manifest
        .store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(store.publish(candidate), Err(StoreError::Unavailable)));
    let reopened = store
        .open_library(&library())
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("cache miss"));
    assert_eq!(get(&old, "record:one"), get(&reopened, "record:one"));
    store
        .inner
        .fail_manifest
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let recovered = store
        .publish(stage(&store, "After failed manifest"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(title(&get(&recovered, "record:one")), "After failed manifest");
}

#[test]
fn close_reopen_retires_old_operations_and_preserves_accepted_disk_data() {
    let (directory, store) = open();
    let accepted = store
        .publish(stage(&store, "Accepted"))
        .unwrap_or_else(|e| panic!("{e}"));
    let old_operation = stage(&store, "Retired");
    store.close().unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(store.publish(old_operation), Err(StoreError::Closed)));
    assert_eq!(title(&get(&accepted, "record:one")), "Accepted");
    drop(accepted);
    drop(store);
    let reopened = GenerationStore::open(&directory.path().join("generations")).unwrap_or_else(|e| panic!("{e}"));
    let generation = reopened
        .open_library(&library())
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("cache miss"));
    assert_eq!(title(&get(&generation, "record:one")), "Accepted");
}

#[test]
fn unfinished_writer_does_not_keep_publication_authority_alive_after_reopen() {
    let (directory, store) = open();
    let mut old = store
        .begin(library(), Fingerprint::of_bytes(b"old session"))
        .unwrap_or_else(|e| panic!("{e}"));
    old.push(item("record:one", "Retired"))
        .unwrap_or_else(|e| panic!("{e}"));
    store.close().unwrap_or_else(|e| panic!("{e}"));
    drop(store);
    let reopened = GenerationStore::open(&directory.path().join("generations")).unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(old.finish(), Err(StoreError::Closed)));
    assert!(
        reopened
            .open_library(&library())
            .unwrap_or_else(|e| panic!("{e}"))
            .is_none()
    );
}

#[test]
fn cache_lease_contention_refuses_a_second_writer() {
    let (directory, store) = open();
    assert!(matches!(
        GenerationStore::open(&directory.path().join("generations")),
        Err(StoreError::Busy)
    ));
    drop(store);
    assert!(GenerationStore::open(&directory.path().join("generations")).is_ok());
}

#[test]
fn a_replaced_lease_refuses_old_publication_and_cleanup() {
    let (directory, store) = open();
    let accepted = store
        .publish(stage(&store, "Accepted"))
        .unwrap_or_else(|e| panic!("{e}"));
    let delayed = stage(&store, "Retired");
    let root = directory.path().join("generations");
    let orphan = root.join("generation-orphan");
    std::fs::write(&orphan, MAGIC).unwrap_or_else(|e| panic!("{e}"));
    std::fs::rename(root.join(".lease"), root.join("retired-lease")).unwrap_or_else(|e| panic!("{e}"));
    let current = GenerationStore::open(&root).unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(store.publish(delayed), Err(StoreError::Unavailable)));
    assert!(matches!(
        store.begin(library(), Fingerprint::of_bytes(b"retired")),
        Err(StoreError::Unavailable)
    ));
    assert!(orphan.exists());
    let reopened = current
        .open_library(&library())
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("cache miss"));
    assert_eq!(get(&accepted, "record:one"), get(&reopened, "record:one"));
}

fn rewrite_index(store: &GenerationStore, change: impl FnOnce(&mut Index)) {
    let path = store.inner.directory.path().join(manifest_name(&library()));
    let mut manifest: Manifest = serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    let data_path = store.inner.directory.path().join(&manifest.file);
    let mut data = std::fs::read(&data_path).unwrap_or_else(|e| panic!("{e}"));
    let mut index: Index =
        serde_json::from_slice(&data[manifest.index_start as usize..]).unwrap_or_else(|e| panic!("{e}"));
    change(&mut index);
    let encoded = serde_json::to_vec(&index).unwrap_or_else(|e| panic!("{e}"));
    data.truncate(manifest.index_start as usize);
    data.extend(&encoded);
    manifest.index_len = encoded.len() as u64;
    manifest.index_hash = Fingerprint::of_bytes(&encoded);
    std::fs::write(data_path, data).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(path, serde_json::to_vec(&manifest).unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn checked_metadata_rejects_malformed_bounds_duplicates_and_identity_even_with_a_valid_checksum() {
    for case in 0..4 {
        let (_directory, store) = open();
        drop(
            store
                .publish(stage(&store, "Accepted"))
                .unwrap_or_else(|e| panic!("{e}")),
        );
        rewrite_index(&store, |index| match case {
            0 => index.rows[0].offset = u64::MAX,
            1 => index.rows[0].len = u64::MAX,
            2 => index.rows.push(index.rows[0].clone()),
            _ => index.rows[0].id.library = LibraryId::file(b"different invented source"),
        });
        assert!(matches!(store.open_library(&library()), Err(StoreError::Invalid)));
    }
}

#[test]
fn forged_fingerprints_are_recomputed_when_a_selected_object_is_materialized() {
    use std::io::{Seek, SeekFrom, Write};
    let (_directory, store) = open();
    let generation = store
        .publish(stage(&store, "Accepted"))
        .unwrap_or_else(|e| panic!("{e}"));
    let row = &generation.index.rows[0];
    let data = std::fs::read(&generation.path).unwrap_or_else(|e| panic!("{e}"));
    let mut object: SnapshotObject =
        serde_json::from_slice(&data[row.offset as usize..(row.offset + row.len) as usize])
            .unwrap_or_else(|e| panic!("{e}"));
    let forged = Fingerprint::of_bytes(b"forged");
    object.fingerprints.cite = forged.clone();
    let encoded = serde_json::to_vec(&object).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(encoded.len() as u64, row.len);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&generation.path)
        .unwrap_or_else(|e| panic!("{e}"));
    file.seek(SeekFrom::Start(row.offset)).unwrap_or_else(|e| panic!("{e}"));
    file.write_all(&encoded).unwrap_or_else(|e| panic!("{e}"));
    drop(generation);
    rewrite_index(&store, |index| {
        index.rows[0].fingerprints.cite = forged;
        index.rows[0].hash = Fingerprint::of_bytes(&encoded);
    });
    let generation = store
        .open_library(&library())
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("cache miss"));
    assert!(matches!(
        generation.get(&ItemRef {
            library: library(),
            key: "record:one".into()
        }),
        Err(StoreError::Invalid)
    ));
}

#[test]
fn rejected_stream_input_cannot_be_ignored_to_publish_a_partial_generation() {
    let (_directory, store) = open();
    let mut writer = store
        .begin(library(), Fingerprint::of_bytes(b"source"))
        .unwrap_or_else(|e| panic!("{e}"));
    writer
        .push(item("record:one", "Accepted prefix"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(
        writer.push(item("record:one", "Duplicate")),
        Err(StoreError::Invalid)
    ));
    assert!(matches!(writer.finish(), Err(StoreError::Invalid)));
    assert!(
        store
            .open_library(&library())
            .unwrap_or_else(|e| panic!("{e}"))
            .is_none()
    );
}

#[test]
fn unselected_corrupt_payload_is_not_deserialized_to_open_or_read_another_object() {
    use std::io::{Seek, SeekFrom, Write};
    let (_directory, store) = open();
    let mut writer = store
        .begin(library(), Fingerprint::of_bytes(b"source"))
        .unwrap_or_else(|e| panic!("{e}"));
    writer
        .push(item("record:a", "Selected"))
        .unwrap_or_else(|e| panic!("{e}"));
    writer
        .push(item("record:z", "Corrupt unselected"))
        .unwrap_or_else(|e| panic!("{e}"));
    let generation = store
        .publish(writer.finish().unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    let row = &generation.index.rows[1];
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&generation.path)
        .unwrap_or_else(|e| panic!("{e}"));
    file.seek(SeekFrom::Start(row.offset)).unwrap_or_else(|e| panic!("{e}"));
    file.write_all(b"!").unwrap_or_else(|e| panic!("{e}"));
    drop(generation);
    let reopened = store
        .open_library(&library())
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("cache miss"));
    assert_eq!(title(&get(&reopened, "record:a")), "Selected");
    assert!(matches!(
        reopened.get(&ItemRef {
            library: library(),
            key: "record:z".into()
        }),
        Err(StoreError::Invalid)
    ));
}

#[test]
fn unknown_format_is_a_cache_miss_and_truncated_or_corrupt_files_are_refused() {
    let (_directory, store) = open();
    drop(
        store
            .publish(stage(&store, "Accepted"))
            .unwrap_or_else(|e| panic!("{e}")),
    );
    let path = store.inner.directory.path().join(manifest_name(&library()));
    let mut manifest: Manifest = serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    manifest.format += 1;
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        store
            .open_library(&library())
            .unwrap_or_else(|e| panic!("{e}"))
            .is_none()
    );
    std::fs::write(path, b"{truncated").unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(store.open_library(&library()), Err(StoreError::Invalid)));
}

#[test]
fn oversized_metadata_sequences_are_refused_during_decode() {
    let mut input = serde_json::Deserializer::from_str("[1,2,3]");
    assert!(bounded_sequence::<_, u8>(&mut input, 2).is_err());
    for authors in [true, false] {
        let (_directory, store) = open();
        drop(
            store
                .publish(stage(&store, "Accepted"))
                .unwrap_or_else(|e| panic!("{e}")),
        );
        rewrite_index(&store, |index| {
            if authors {
                index.rows[0]
                    .summary
                    .as_mut()
                    .unwrap_or_else(|| panic!("missing summary"))
                    .authors = vec![String::new(); 257];
            } else {
                index.rows[0].keys = vec![String::new(); 257];
            }
        });
        assert!(matches!(store.open_library(&library()), Err(StoreError::Invalid)));
    }
}

const METADATA_SLOT_LIMIT: usize = 1_000_000;

fn dense_item(position: usize, slots: usize) -> LibraryObject {
    let mut object = item(&format!("record:{position:05}"), "Dense");
    if let LibraryObject::Record(record) = &mut object {
        record.aliases = vec![String::new(); slots.min(256)];
        record.csl.insert(
            "author".into(),
            json!(vec![json!({"literal":""}); slots.saturating_sub(256)]),
        );
    }
    object
}

#[test]
fn aggregate_metadata_writer_boundary_reopens_and_overflow_preserves_the_manifest() {
    let (_directory, store) = open();
    let build = |slots: usize| {
        let mut writer = store
            .begin(library(), Fingerprint::of_bytes(b"dense source"))
            .unwrap_or_else(|e| panic!("{e}"));
        for position in 0..slots.div_ceil(512) {
            writer
                .push(dense_item(position, (slots - position * 512).min(512)))
                .unwrap_or_else(|e| panic!("{e}"));
        }
        writer
    };
    let accepted = store
        .publish(build(METADATA_SLOT_LIMIT).finish().unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    let reopened = store
        .open_library(&library())
        .unwrap_or_else(|e| panic!("{e}"))
        .unwrap_or_else(|| panic!("cache miss"));
    assert_eq!(reopened.summaries().count(), METADATA_SLOT_LIMIT.div_ceil(512));
    assert_eq!(get(&accepted, "record:00000"), get(&reopened, "record:00000"));
    let manifest = store.inner.directory.path().join(manifest_name(&library()));
    let previous = std::fs::read(&manifest).unwrap_or_else(|e| panic!("{e}"));
    let mut rejected = build(METADATA_SLOT_LIMIT);
    assert!(matches!(rejected.push(dense_item(99999, 1)), Err(StoreError::Limit)));
    assert!(matches!(rejected.finish(), Err(StoreError::Invalid)));
    assert_eq!(std::fs::read(manifest).unwrap_or_else(|e| panic!("{e}")), previous);
}

#[test]
fn aggregate_metadata_decoder_refuses_many_individually_bounded_rows() {
    let (_directory, store) = open();
    drop(
        store
            .publish(stage(&store, &"x".repeat(4096)))
            .unwrap_or_else(|e| panic!("{e}")),
    );
    rewrite_index(&store, |index| {
        let mut template = index.rows[0].clone();
        template.keys = vec![String::new(); 256];
        let summary = template.summary.as_mut().unwrap_or_else(|| panic!("missing summary"));
        summary.title.clear();
        summary.authors = vec![String::new(); 256];
        index.rows = (0..=METADATA_SLOT_LIMIT / 512)
            .map(|position| {
                let mut row = template.clone();
                row.id.key = format!("record:{position:05}");
                row.offset = MAGIC.len() as u64 + position as u64;
                row.len = 1;
                row
            })
            .collect();
    });
    assert!(matches!(store.open_library(&library()), Err(StoreError::Invalid)));
}

#[test]
fn empty_configured_library_can_be_saved_and_reopened() {
    let (_directory, store) = open();
    let writer = store
        .begin(library(), Fingerprint::of_bytes(b"empty source"))
        .unwrap_or_else(|e| panic!("{e}"));
    let generation = store
        .publish(writer.finish().unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(generation.summaries().count(), 0);
    assert_eq!(generation.library(), &library());
    assert_eq!(generation.observation(), &Fingerprint::of_bytes(b"empty source"));
    assert!(
        store
            .open_library(&library())
            .unwrap_or_else(|e| panic!("{e}"))
            .is_some()
    );
}

#[cfg(unix)]
#[test]
fn unsafe_file_modes_and_links_refuse_reopen_without_exposing_paths() {
    use std::os::unix::fs::PermissionsExt;
    for kind in 0..3 {
        let (_directory, store) = open();
        let generation = store
            .publish(stage(&store, "Private invented title"))
            .unwrap_or_else(|e| panic!("{e}"));
        if kind == 1 {
            std::fs::hard_link(&generation.path, generation.path.with_extension("foreign"))
                .unwrap_or_else(|e| panic!("{e}"));
        } else if kind == 2 {
            let outside = generation.path.with_extension("foreign");
            std::fs::rename(&generation.path, &outside).unwrap_or_else(|e| panic!("{e}"));
            std::os::unix::fs::symlink(outside, &generation.path).unwrap_or_else(|e| panic!("{e}"));
        } else {
            std::fs::set_permissions(&generation.path, std::fs::Permissions::from_mode(0o644))
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let error = store
            .open_library(&library())
            .err()
            .unwrap_or_else(|| panic!("unsafe file accepted"));
        assert_eq!(error, StoreError::Unavailable);
        assert!(!error.to_string().contains("Private invented title"));
        assert!(
            !error
                .to_string()
                .contains(&*store.inner.directory.path().to_string_lossy())
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn extended_file_acl_refuses_reopen() {
    let (_directory, store) = open();
    let generation = store
        .publish(stage(&store, "Accepted"))
        .unwrap_or_else(|e| panic!("{e}"));
    let status = std::process::Command::new("/bin/chmod")
        .args(["+a", "everyone allow read"])
        .arg(&generation.path)
        .status()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(status.success());
    assert!(matches!(store.open_library(&library()), Err(StoreError::Unavailable)));
}

#[cfg(windows)]
#[test]
fn extended_file_acl_refuses_reopen() {
    let (_directory, store) = open();
    let generation = store
        .publish(stage(&store, "Accepted"))
        .unwrap_or_else(|e| panic!("{e}"));
    let status = std::process::Command::new("icacls")
        .arg(&generation.path)
        .args(["/grant", "*S-1-1-0:(R)"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap_or_else(|_| panic!("ACL fixture unavailable"));
    assert!(status.success(), "ACL fixture failed");
    assert!(matches!(store.open_library(&library()), Err(StoreError::Unavailable)));
}
