//! Invented API responses; never connects to Zotero or discovers a profile.
#![allow(clippy::unwrap_used)]
use ibis_core::zotero::api::{Response, Transport, TransportError};
use ibis_zotero_local::live::{Library, Limits, LiveError, LiveReader, LiveWarning, Refresh};
use serde_json::{Value, json};
use std::collections::BTreeMap;
struct Fake {
    major: u8,
    version: u64,
    instance: &'static str,
    rows: Vec<Value>,
    response_rows: Option<Vec<Value>>,
    paths: Vec<String>,
    fail: bool,
}
impl Fake {
    fn new(major: u8) -> Self {
        Self {
            major,
            version: 1,
            instance: "synthetic-a",
            rows: vec![row(1, "Invented atlas")],
            response_rows: None,
            paths: vec![],
            fail: false,
        }
    }
}
fn row(version: u64, title: &str) -> Value {
    json!({"key":"SYNTH001","version":version,"data":{"key":"SYNTH001","version":version,
        "itemType":"book","title":title,"citationKey":"Synthetic2026","extra":"ordinary raw Extra marker",
        "creators":[],"dateModified":"2026-10-26T12:00:00Z"}})
}
impl Transport for Fake {
    fn get(&mut self, path: &str) -> Result<Response, TransportError> {
        self.paths.push(path.into());
        if self.fail {
            return Err(TransportError::Unreachable);
        }
        let body = if path.ends_with("format=versions") {
            json!(
                self.rows
                    .iter()
                    .map(|row| (row["key"].as_str().unwrap(), row["version"].clone()))
                    .collect::<BTreeMap<_, _>>()
            )
        } else {
            assert!(path.contains("include=data"));
            assert!(!path.contains("csljson"));
            json!(self.response_rows.as_ref().unwrap_or(&self.rows))
        };
        Ok(Response {
            status: 200,
            body: serde_json::to_vec(&body).unwrap(),
            headers: vec![
                ("Zotero-API-Version".into(), "3".into()),
                ("X-Zotero-Version".into(), format!("{}.0", self.major)),
                ("Zotero-Server-ID".into(), self.instance.into()),
                ("Last-Modified-Version".into(), self.version.to_string()),
                ("Total-Results".into(), self.rows.len().to_string()),
            ],
        })
    }
}
#[test]
fn ten_reconciles_versions_deletions_and_full_reload_without_losing_raw_extra() {
    let mut fake = Fake::new(10);
    let mut reader = LiveReader::new(Library::User);
    assert_eq!(reader.refresh(&mut fake, false, &|| false), Ok(Refresh::Changed));
    assert_eq!(reader.items()[0].fields["extra"], "ordinary raw Extra marker");
    fake.paths.clear();
    assert_eq!(reader.refresh(&mut fake, false, &|| false), Ok(Refresh::Unchanged));
    assert_eq!(fake.paths.len(), 2);
    fake.rows = vec![row(2, "Edited atlas")];
    fake.version = 2;
    reader.refresh(&mut fake, false, &|| false).unwrap();
    assert_eq!(reader.items()[0].fields["title"], "Edited atlas");
    fake.paths.clear();
    reader.refresh(&mut fake, true, &|| false).unwrap();
    assert!(fake.paths.iter().any(|path| path.contains("include=data")));
    fake.rows.clear();
    fake.version = 3;
    reader.refresh(&mut fake, false, &|| false).unwrap();
    assert!(reader.items().is_empty());
}
#[test]
fn older_servers_and_changed_instances_reread_on_doubt() {
    for major in 7..=10 {
        let mut fake = Fake::new(major);
        let mut reader = LiveReader::new(Library::User);
        reader.refresh(&mut fake, false, &|| false).unwrap();
        fake.rows = vec![row(1, "Backdated imported atlas")];
        if major == 10 {
            fake.instance = "synthetic-b";
        }
        reader.refresh(&mut fake, false, &|| false).unwrap();
        assert_eq!(reader.items()[0].fields["title"], "Backdated imported atlas");
    }
}
#[test]
fn failure_cancellation_and_limits_keep_last_good() {
    let mut fake = Fake::new(10);
    let mut reader = LiveReader::new(Library::User);
    reader.refresh(&mut fake, false, &|| false).unwrap();
    fake.fail = true;
    assert_eq!(
        reader.refresh(&mut fake, false, &|| false),
        Err(LiveError::Transport(TransportError::Unreachable))
    );
    assert_eq!(reader.refresh(&mut fake, false, &|| true), Err(LiveError::Cancelled));
    assert_eq!(reader.items()[0].fields["title"], "Invented atlas");
    let mut reader = LiveReader::with_limits(
        Library::User,
        Limits {
            requests: 0,
            ..Limits::default()
        },
    );
    fake.fail = false;
    assert_eq!(reader.refresh(&mut fake, false, &|| false), Err(LiveError::Limit));
    assert!(reader.items().is_empty());
    let mut reader = LiveReader::with_limits(
        Library::User,
        Limits {
            bytes: 1,
            ..Limits::default()
        },
    );
    assert_eq!(reader.refresh(&mut fake, false, &|| false), Err(LiveError::Limit));
    assert!(reader.items().is_empty());
}

#[test]
fn a_changed_scan_retries_from_full_data_even_when_versions_return_to_cached_values() {
    let mut fake = Fake::new(10);
    let mut reader = LiveReader::new(Library::User);
    reader.refresh(&mut fake, false, &|| false).unwrap();
    fake.rows = vec![row(1, "Restored synthetic atlas")];
    struct Drifting {
        inner: Fake,
        inventories: usize,
    }
    impl Transport for Drifting {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            if path.ends_with("format=versions") {
                self.inventories += 1;
            }
            self.inner.version = if self.inventories == 2 { 2 } else { 1 };
            self.inner.rows = vec![row(self.inner.version, "Restored synthetic atlas")];
            self.inner.get(path)
        }
    }
    let mut drifting = Drifting {
        inner: fake,
        inventories: 0,
    };
    reader.refresh(&mut drifting, false, &|| false).unwrap();
    assert_eq!(reader.items()[0].fields["title"], "Restored synthetic atlas");
}

#[test]
fn unrelated_library_writes_do_not_starve_a_stable_ten_item_map() {
    struct Churning(Fake);
    impl Transport for Churning {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            self.0.version += 1;
            self.0.get(path)
        }
    }
    let mut reader = LiveReader::new(Library::User);
    reader.refresh(&mut Churning(Fake::new(10)), false, &|| false).unwrap();
    assert_eq!(reader.items()[0].fields["title"], "Invented atlas");
}

#[test]
fn explicit_full_reload_retires_preview_before_the_first_request() {
    let began = std::cell::Cell::new(false);
    let mut reader = LiveReader::new(Library::User);
    let mut fake = Fake::new(10);
    reader.refresh(&mut fake, false, &|| false).unwrap();
    struct Guarded<'a>(&'a std::cell::Cell<bool>, Fake);
    impl Transport for Guarded<'_> {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            assert!(self.0.get(), "full read must retire the old preview before I/O");
            self.1.get(path)
        }
    }
    reader
        .refresh_observed(&mut Guarded(&began, fake), true, &|| false, &mut || {
            began.set(true);
            Ok(())
        })
        .unwrap();
}

#[test]
fn per_item_limits_omit_only_the_oversized_item_and_keep_its_warning_across_refresh() {
    let mut cases = vec![];
    let mut authors = row(1, "Invented collaboration");
    authors["data"]["creators"] = json!(vec![
        json!({"creatorType":"author","name":"Invented collaborator"});
        4097
    ]);
    cases.push(authors);
    let mut name = row(1, "Invented long name");
    name["data"]["creators"] = json!([{"creatorType":"author","name":"x".repeat(300_000)}]);
    cases.push(name);
    let mut field = row(1, "Invented field");
    field["data"]["title"] = json!("x".repeat(300_000));
    cases.push(field);
    let mut count = row(1, "Invented fields");
    for i in 0..257 {
        count["data"]
            .as_object_mut()
            .unwrap()
            .insert(format!("invented{i}"), json!("x"));
    }
    cases.push(count);
    let mut total = row(1, "Invented total");
    for i in 0..9 {
        total["data"]
            .as_object_mut()
            .unwrap()
            .insert(format!("invented{i}"), json!("x".repeat(256 * 1024)));
    }
    cases.push(total);
    for mut large in cases {
        large["key"] = json!("SYNTH002");
        large["data"]["key"] = json!("SYNTH002");
        let mut fake = Fake::new(10);
        fake.rows.push(large);
        let mut reader = LiveReader::new(Library::User);
        reader.refresh(&mut fake, false, &|| false).unwrap();
        assert_eq!(reader.items().len(), 1);
        assert_eq!(reader.warnings().collect::<Vec<_>>(), [LiveWarning::ItemTooLarge]);
        reader.refresh(&mut fake, false, &|| false).unwrap();
        assert_eq!(reader.warnings().collect::<Vec<_>>(), [LiveWarning::ItemTooLarge]);
    }
}

#[test]
fn malformed_rows_with_or_without_keys_keep_neighbors_and_cached_warnings() {
    for data in [
        json!(null),
        json!({}),
        json!({"key": "SYNTH002", "version": 1, "itemType": "book"}),
    ] {
        for keyed in [true, false] {
            let mut fake = Fake::new(10);
            fake.rows.push(json!({"key": "SYNTH002", "version": 1, "data": data}));
            let mut rows = fake.rows.clone();
            if !keyed {
                rows[1].as_object_mut().unwrap().remove("key");
                if let Some(data) = rows[1]["data"].as_object_mut() {
                    data.remove("key");
                }
            }
            fake.response_rows = Some(rows);
            let mut reader = LiveReader::new(Library::User);
            for requests in [3, 2] {
                fake.paths.clear();
                reader.refresh(&mut fake, false, &|| false).unwrap();
                assert_eq!(reader.items().len(), 1);
                assert_eq!(reader.items()[0].key, "SYNTH001");
                assert_eq!(reader.warnings().collect::<Vec<_>>(), [LiveWarning::MalformedItem]);
                assert_eq!(reader.retained_size().0, 2);
                assert_eq!(fake.paths.len(), requests);
            }
            let mut repaired = row(2, "Invented repaired item");
            repaired["key"] = json!("SYNTH002");
            repaired["data"]["key"] = json!("SYNTH002");
            fake.rows = vec![repaired];
            fake.response_rows = None;
            fake.version = 2;
            reader.refresh(&mut fake, false, &|| false).unwrap();
            assert_eq!(reader.items()[0].fields["title"], "Invented repaired item");
            assert!(reader.warnings().next().is_none());
            // A newly malformed anonymous row must evict the cached record.
            fake.rows = vec![json!({"key": "SYNTH002", "version": 3, "data": null})];
            fake.response_rows = Some(vec![json!({})]);
            fake.version = 3;
            reader.refresh(&mut fake, false, &|| false).unwrap();
            assert!(reader.items().is_empty());
            assert_eq!(reader.retained_size().0, 1);
            assert_eq!(reader.warnings().collect::<Vec<_>>(), [LiveWarning::MalformedItem]);
        }
    }
}

#[test]
fn anonymous_rows_match_remaining_inventory_versions_without_assuming_order() {
    for versions in [[Some(2), Some(1)], [None, None], [Some(2), Some(2)]] {
        let mut fake = Fake::new(10);
        let mut second = row(2, "Invented second item");
        second["key"] = json!("SYNTH002");
        second["data"]["key"] = json!("SYNTH002");
        fake.rows.push(second);
        fake.version = 2;
        fake.response_rows = Some(versions.iter().map(|version| json!({"version": version})).collect());
        let mut reader = LiveReader::new(Library::User);
        let result = reader.refresh(&mut fake, false, &|| false);
        if versions == [Some(2), Some(2)] {
            assert_eq!(result, Err(LiveError::Changing));
            assert!(reader.warnings().next().is_none());
        } else {
            assert_eq!(result, Ok(Refresh::Changed));
            assert_eq!(reader.retained_size().0, 2);
            assert_eq!(reader.warnings().count(), 2);
        }
        assert!(reader.items().is_empty());
    }
}

#[test]
fn anonymous_rows_cannot_hide_wrong_versions_or_incomplete_or_duplicate_pages() {
    for rows in [vec![], vec![json!({"version": 2})], vec![row(1, "Invented atlas"); 2]] {
        let mut fake = Fake::new(10);
        fake.response_rows = Some(rows);
        let mut reader = LiveReader::new(Library::User);
        assert_eq!(reader.refresh(&mut fake, false, &|| false), Err(LiveError::Changing));
        assert!(reader.items().is_empty());
        assert!(fake.paths.len() <= 6);
    }
}
