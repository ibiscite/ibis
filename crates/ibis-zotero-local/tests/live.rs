//! Invented API responses; never connects to Zotero or discovers a profile.
#![allow(clippy::unwrap_used)]
use ibis_core::zotero::api::{Response, Transport, TransportError};
use ibis_zotero_local::live::{Library, Limits, LiveError, LiveReader, Refresh};
use serde_json::{Value, json};
use std::collections::BTreeMap;
struct Fake {
    major: u8,
    version: u64,
    instance: &'static str,
    rows: Vec<Value>,
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
            json!(self.rows)
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
