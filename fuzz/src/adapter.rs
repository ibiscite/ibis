//! Source-adapter oracles (decision 0056): what an outside program writes,
//! and what the built-in EndNote adapter makes of an exported XML file.
//!
//! An adapter is any program the person named, so its answer is untrusted:
//! any bytes give a library Ibis can publish or a fixed error. The EndNote
//! adapter reads a file the person chose; whatever it answers, its parent
//! accepts it, and it never answers outside the protocol.

use std::time::Instant;

use ibis_core::contract::LibraryObject;
use ibis_core::contract::adapter::{ADAPTER_PROTOCOL, AdapterMethod, AdapterRequest, AdapterResponse};
use ibis_sources::adapter::{MAX_TOKEN_CHARS, SourceAdapter};

use crate::{
    MAX_INPUT_BYTES, assert_in_time, assert_no_contained_panic, assert_snapshot_rebuilds, assert_wire_round_trip,
    watch_panics,
};

/// An approved adapter whose program is never started: only its answer is read.
fn adapter() -> SourceAdapter {
    let program = std::env::current_exe().unwrap_or_else(|_| panic!("no path for this program"));
    SourceAdapter::at("invented", program).unwrap_or_else(|_| panic!("the invented adapter was refused"))
}

/// `stdout` as one adapter's complete answer.
pub fn check_adapter_answer(stdout: &[u8]) {
    if stdout.len() > MAX_INPUT_BYTES {
        return;
    }
    watch_panics();
    let started = Instant::now();
    let result = adapter().answer(stdout);
    assert_no_contained_panic();
    if let Ok(load) = result {
        assert!(
            load.token.chars().count() <= MAX_TOKEN_CHARS,
            "a revision past its bound"
        );
        for item in load.loaded.snapshot.objects() {
            assert_eq!(item.object.id().library, load.loaded.library, "wrong library identity");
            if let LibraryObject::Record(record) = &item.object {
                assert_wire_round_trip(record);
            }
        }
        assert_snapshot_rebuilds(&load.loaded.snapshot);
    }
    assert_in_time(started);
}

/// `xml` as an EndNote XML export, read by `ibis __source endnote`.
pub fn check_endnote_export(xml: &[u8]) {
    if xml.len() > MAX_INPUT_BYTES {
        return;
    }
    watch_panics();
    let started = Instant::now();
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("temporary directory creation failed"));
    let path = directory.path().join("invented.xml");
    std::fs::write(&path, xml).unwrap_or_else(|_| panic!("temporary input write failed"));
    let request = serde_json::to_vec(&AdapterRequest {
        ibis: ADAPTER_PROTOCOL,
        method: AdapterMethod::Read,
        library: Some(path.to_string_lossy().into_owned()),
    })
    .unwrap_or_else(|_| panic!("request serialization failed"));
    let mut stdout = Vec::new();
    let status = ibis_sources::endnote::serve(request.as_slice(), &mut stdout);
    assert_no_contained_panic();
    let response: AdapterResponse =
        serde_json::from_slice(&stdout).unwrap_or_else(|_| panic!("the adapter answered outside the protocol"));
    assert_eq!(response.ibis, ADAPTER_PROTOCOL);
    assert!(
        response.read.is_some() != response.refused.is_some(),
        "an answer must read or refuse"
    );
    if response.read.is_some() {
        assert_eq!(status, 0, "a read with a failing exit status");
        match adapter().answer(&stdout) {
            Ok(load) => assert_snapshot_rebuilds(&load.loaded.snapshot),
            Err(error) => panic!("the parent refused the EndNote adapter's read: {error:?}"),
        }
    } else {
        assert!(adapter().answer(&stdout).is_err(), "a refusal became a library");
    }
    assert_no_contained_panic();
    assert_in_time(started);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seeds reach the record reader, not an early refusal.
    #[test]
    fn seeds_reach_the_readers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
        let answer = std::fs::read(root.join("adapter_answer/read.json")).unwrap_or_else(|_| panic!("no seed"));
        assert!(
            adapter()
                .answer(&answer)
                .is_ok_and(|load| !load.loaded.snapshot.objects().is_empty())
        );
    }
}
