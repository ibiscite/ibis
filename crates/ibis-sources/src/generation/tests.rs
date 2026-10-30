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
        identifiers: vec![],
    })
}

fn open() -> (tempfile::TempDir, GenerationStore) {
    let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let store = GenerationStore::open(&directory.path().join("generations")).unwrap_or_else(|e| panic!("{e}"));
    (directory, store)
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
        generation.summaries().map(|row| row.title.as_str()).collect::<Vec<_>>(),
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
    let (_directory, store) = open();
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
        index.rows[0].fingerprints.cite = forged.clone();
        index.rows[0]
            .summary
            .as_mut()
            .unwrap_or_else(|| panic!("missing summary"))
            .fingerprints
            .cite = forged;
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
