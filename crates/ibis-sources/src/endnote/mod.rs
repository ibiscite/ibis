//! EndNote libraries for Ibis: the first source adapter (decision 0056,
//! step 4). It speaks only the adapter protocol and is served by
//! `ibis __source endnote`, so it ships wherever `ibis` does.
//!
//! Two routes share one reader and one field mapping, both of EndNote's own
//! XML:
//!
//! - **An open library** (macOS): asked of EndNote through its scripting
//!   interface. Record ids are EndNote's record numbers, which EndNote never
//!   reuses within a library. The scope is the library's EndNote id, its
//!   path and the identity of its `.Data` folder, so a copied, restored or
//!   moved library gets new handles.
//! - **An exported `.xml` file** (every system): record ids are digests of
//!   what each record says, as for RIS, because an export cannot show that
//!   a number still names the same record.
//!
//! EndNote has no citation keys: its Label field holds whatever a person
//! typed, so no key is claimed. Attachments, notes and abstracts are not read.
#[cfg(target_os = "macos")]
mod live;
#[cfg(any(target_os = "macos", test))]
mod macroman;
mod mapping;
mod xml;

use std::io::{Read, Write};
use std::path::Path;

use ibis_core::contract::Fingerprint;
use ibis_core::contract::adapter::{
    ADAPTER_PROTOCOL, AdapterRead, AdapterRefusal, AdapterRequest, AdapterResponse, AdapterSkip, AdapterSkipCause,
};
use serde_json::{Map, Value, json};

/// The largest request read.
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
/// The largest export file read: the bound on library files.
const MAX_FILE_BYTES: u64 = crate::MAX_LIBRARY_BYTES;

/// Answer one request from `input` on `output`; the exit status.
pub fn serve(input: impl Read, mut output: impl Write) -> u8 {
    let answer = match crate::child::read_bounded(input, MAX_REQUEST_BYTES) {
        Ok(request) => match serde_json::from_slice::<AdapterRequest>(&request) {
            Ok(request) if request.ibis == ADAPTER_PROTOCOL => answer(request.library.as_deref()),
            _ => Err(AdapterRefusal::Unsupported),
        },
        _ => Err(AdapterRefusal::Unsupported),
    };
    let response = match answer {
        Ok(read) => AdapterResponse {
            ibis: ADAPTER_PROTOCOL,
            read: Some(read),
            refused: None,
        },
        Err(refusal) => AdapterResponse {
            ibis: ADAPTER_PROTOCOL,
            read: None,
            refused: Some(refusal),
        },
    };
    match serde_json::to_writer(&mut output, &response).map(|()| output.flush()) {
        Ok(Ok(())) => 0,
        _ => 1,
    }
}

fn answer(library: Option<&str>) -> Result<AdapterRead, AdapterRefusal> {
    match library {
        Some(file) if file.to_ascii_lowercase().ends_with(".xml") => exported(Path::new(file)),
        #[cfg(target_os = "macos")]
        library => live::read(library),
        #[cfg(not(target_os = "macos"))]
        _ => Err(AdapterRefusal::Unsupported),
    }
}

/// An exported XML file, by its path.
#[allow(
    clippy::disallowed_methods,
    reason = "The adapter runs in its own process; this one bounded read is its only file access."
)]
fn exported(path: &Path) -> Result<AdapterRead, AdapterRefusal> {
    let path = path.canonicalize().map_err(|_| AdapterRefusal::LibraryNotFound)?;
    let file = std::fs::File::open(&path).map_err(|_| AdapterRefusal::LibraryNotFound)?;
    let metadata = file.metadata().map_err(|_| AdapterRefusal::Unreadable)?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(AdapterRefusal::Unreadable);
    }
    let bytes = crate::child::read_bounded(file, MAX_FILE_BYTES).map_err(|_| AdapterRefusal::Unreadable)?;
    let text = String::from_utf8(bytes).map_err(|_| AdapterRefusal::Unreadable)?;
    let records = xml::records(&text).map_err(|_| AdapterRefusal::Unreadable)?;
    let scope = Fingerprint::of_bytes(&[b"xml\0", path.as_os_str().as_encoded_bytes()].concat());
    Ok(read(scope, records.iter().map(|record| (None, record))))
}

/// Mapped records, each with its EndNote record number when that is its
/// identity; otherwise identified by what it says.
fn read<'a>(scope: Fingerprint, records: impl Iterator<Item = (Option<String>, &'a xml::XmlRecord)>) -> AdapterRead {
    let mut kept: Vec<(String, Map<String, Value>)> = Vec::new();
    let mut skipped = Vec::new();
    for (number, record) in records {
        match mapping::csl(record) {
            Ok(csl) => {
                let id = number.unwrap_or_else(|| {
                    let fields = serde_json::to_vec(&csl).unwrap_or_default();
                    format!("content-{}", Fingerprint::of_bytes(&fields).as_str())
                });
                kept.push((id, csl));
            }
            Err(skip) => skipped.push(skip),
        }
    }
    // The same records give the same revision, whatever order EndNote lists them in.
    kept.sort_by(|a, b| a.0.cmp(&b.0));
    let revision = Fingerprint::of_bytes(&serde_json::to_vec(&kept).unwrap_or_default());
    AdapterRead {
        scope: scope.as_str().to_owned(),
        revision: revision.as_str().to_owned(),
        keys: false,
        records: kept
            .into_iter()
            .map(|(id, mut csl)| {
                csl.insert("id".into(), json!(id));
                Value::Object(csl)
            })
            .collect(),
        skipped,
    }
}

/// A record the live route cannot place in its library.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn unplaced(record: &xml::XmlRecord) -> AdapterSkip {
    AdapterSkip {
        cause: AdapterSkipCause::Unreadable,
        entry: record.one("titles/title").map(str::to_owned),
        field: Some("rec-number".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(request: &str) -> Value {
        let mut output = Vec::new();
        assert_eq!(serve(request.as_bytes(), &mut output), 0);
        serde_json::from_slice(&output).unwrap_or_default()
    }

    #[test]
    fn an_export_reads_by_content_and_rereads_the_same() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("Invented export.xml");
        let xml = include_str!("../../../../fixtures/endnote/invented.xml");
        std::fs::write(&path, xml).unwrap_or_else(|error| panic!("{error}"));
        let request = json!({"ibis": 1, "method": "read", "library": path}).to_string();
        let first = exchange(&request);
        assert_eq!(first["read"]["keys"], false);
        assert_eq!(first["read"]["records"].as_array().map(Vec::len), Some(4));
        assert_eq!(first["read"]["skipped"][0]["cause"], "unsupported-type");
        assert!(
            first["read"]["records"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|record| record["id"].as_str().is_some_and(|id| id.starts_with("content-")))
        );
        assert_eq!(exchange(&request), first, "a reread is identical");
        std::fs::write(&path, xml.replace("<volume>12</volume>", "<volume>13</volume>"))
            .unwrap_or_else(|error| panic!("{error}"));
        let edited = exchange(&request);
        assert_ne!(edited["read"]["revision"], first["read"]["revision"]);
        assert_eq!(edited["read"]["scope"], first["read"]["scope"]);
        std::fs::write(&path, "<xml><records><record>").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(exchange(&request)["refused"], "unreadable");
        let missing = json!({"ibis": 1, "method": "read", "library": directory.path().join("gone.xml")});
        assert_eq!(exchange(&missing.to_string())["refused"], "library-not-found");
        assert_eq!(
            exchange("{\"ibis\": 2, \"method\": \"read\"}")["refused"],
            "unsupported"
        );
        assert_eq!(exchange("not json")["refused"], "unsupported");
    }
}
