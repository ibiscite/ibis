//! Parser oracles shared by libFuzzer and the ordinary seed-corpus tests, one
//! per untrusted input in `docs/threat-model.md`: library files here, damaged
//! PDFs in [`pdf`], source-adapter answers and EndNote exports in [`adapter`],
//! Zotero's local API and Better BibTeX's key export in [`zotero`]. Every
//! target is listed in [`TARGETS`].
#![allow(clippy::panic, reason = "a fuzz oracle reports a failed property by panicking")]

use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ibis_core::contract::{CitationKeySource, LibraryObject, Record, Snapshot};
use ibis_sources::{SourceError, load_file};

pub mod adapter;
pub mod pdf;
pub mod zotero;

/// A fuzz target: its name (a `fuzz_targets/` file and a `corpus/` folder),
/// its oracle and its largest input.
pub type Target = (&'static str, fn(&[u8]), usize);

/// Every fuzz target. The nightly job runs each; the seed test checks each
/// has a target file and seeds.
pub const TARGETS: &[Target] = &[
    ("damaged_bibtex", check_damaged_bibtex, MAX_INPUT_BYTES),
    ("damaged_csl_json", check_damaged_csl_json, MAX_INPUT_BYTES),
    ("damaged_ris", check_damaged_ris, MAX_INPUT_BYTES),
    ("damaged_csv", check_damaged_csv, MAX_INPUT_BYTES),
    ("damaged_pdf", pdf::check_damaged_pdf, pdf::MAX_INPUT_BYTES),
    ("adapter_answer", adapter::check_adapter_answer, MAX_INPUT_BYTES),
    ("endnote_export", adapter::check_endnote_export, MAX_INPUT_BYTES),
    ("zotero_live", zotero::check_zotero_live, MAX_INPUT_BYTES),
    ("bbt_keys", zotero::check_bbt_keys, MAX_INPUT_BYTES),
];

pub(crate) const MAX_INPUT_BYTES: usize = 64 * 1024;
pub(crate) const INPUT_TIMEOUT: Duration = Duration::from_secs(5);
static INSTALL_HOOK: Once = Once::new();
static PANICKED: AtomicBool = AtomicBool::new(false);

/// Install after the production dispatcher so even a parser panic that it
/// contains is visible. Keep libFuzzer's abort hook for other panics.
/// Run serially: this standalone harness observes process-wide parser panics.
pub(crate) fn watch_panics() {
    INSTALL_HOOK.call_once(|| {
        ibis_core::panic_guard::attempt(|| ()).unwrap_or_else(|_| panic!("panic guard setup failed"));
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            PANICKED.store(true, Ordering::Relaxed);
            previous(info);
        }));
    });
}

pub(crate) fn assert_no_contained_panic() {
    assert!(!PANICKED.load(Ordering::Relaxed), "a parser panic was contained");
}

/// libFuzzer's -timeout also interrupts a parser that never returns. The
/// corpus test uses a watchdog so it can fail on that same regression.
pub(crate) fn assert_in_time(started: Instant) {
    assert!(started.elapsed() < INPUT_TIMEOUT, "input exceeded five seconds");
}

/// Record data must survive serialization without changing meaning.
pub(crate) fn assert_wire_round_trip(record: &Record) {
    let json = serde_json::to_vec(record).unwrap_or_else(|_| panic!("record serialization failed"));
    let round_trip: Record = serde_json::from_slice(&json).unwrap_or_else(|_| panic!("invalid record wire data"));
    assert_eq!(&round_trip, record);
    assert!(
        record
            .csl
            .get("type")
            .is_some_and(|value| value.as_str().is_some_and(|s| !s.is_empty())),
        "a record without a CSL type"
    );
}

/// A published snapshot validates and fingerprints the same when rebuilt.
pub(crate) fn assert_snapshot_rebuilds(snapshot: &Snapshot) {
    let objects = snapshot.objects().iter().map(|item| item.object.clone()).collect();
    let rebuilt = Snapshot::new(objects).unwrap_or_else(|_| panic!("invalid snapshot objects"));
    assert_eq!(&rebuilt, snapshot, "snapshot validation or fingerprints changed");
}

#[derive(Clone, Copy)]
enum Format {
    Bibtex,
    CslJson,
    Ris,
    Csv,
}

/// The real BibTeX loader on raw bytes, including invalid UTF-8.
pub fn check_damaged_bibtex(data: &[u8]) {
    check_library_file(Format::Bibtex, data);
}

/// The CSL-JSON file reader; caller and adapter records use the same reader.
pub fn check_damaged_csl_json(data: &[u8]) {
    check_library_file(Format::CslJson, data);
}

pub fn check_damaged_ris(data: &[u8]) {
    check_library_file(Format::Ris, data);
}

pub fn check_damaged_csv(data: &[u8]) {
    check_library_file(Format::Csv, data);
}

/// The public file loader, chosen by the file's name as Ibis chooses it.
fn check_library_file(format: Format, data: &[u8]) {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    watch_panics();
    let started = Instant::now();
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("temporary directory creation failed"));
    let name = match format {
        Format::Bibtex => "invented.bib",
        Format::CslJson => "invented.json",
        Format::Ris => "invented.ris",
        Format::Csv => "invented.csv",
    };
    let path = directory.path().join(name);
    std::fs::write(&path, data).unwrap_or_else(|_| panic!("temporary input write failed"));
    let result = load_file(&path);
    assert_no_contained_panic();

    match result {
        Ok(loaded) => {
            assert!(std::str::from_utf8(data).is_ok(), "invalid UTF-8 was accepted");
            for item in loaded.snapshot.objects() {
                assert_eq!(item.object.id().library, loaded.library, "wrong library identity");
                if let LibraryObject::Record(record) = &item.object {
                    let sources: Vec<_> = record.citation_keys.iter().map(|key| key.source).collect();
                    match format {
                        Format::Bibtex => {
                            assert!(
                                record.citation_key().is_ok_and(|key| key.is_some()),
                                "invalid BibTeX citation key"
                            );
                            assert!(
                                sources.iter().all(|s| *s == CitationKeySource::Bibtex),
                                "wrong citation key provenance"
                            );
                        }
                        Format::CslJson => assert!(
                            sources.iter().all(|s| *s == CitationKeySource::CslJsonCitationKey),
                            "wrong citation key provenance"
                        ),
                        // Decision 0043: no citation keys from exported files.
                        Format::Ris | Format::Csv => assert!(sources.is_empty(), "a key from an exported file"),
                    }
                    assert_wire_round_trip(record);
                }
            }
            assert_snapshot_rebuilds(&loaded.snapshot);
        }
        // Exported files with a NUL byte are binary, not text (decision 0043).
        Err(SourceError::Unreadable) => assert!(
            std::str::from_utf8(data).is_err() || matches!(format, Format::Ris | Format::Csv) && data.contains(&0),
            "valid text was called unreadable"
        ),
        // Rejecting malformed or unfinished input is allowed; publishing
        // invalid records is not. Other errors indicate a broken harness or an
        // unexpected I/O path.
        Err(SourceError::InvalidData | SourceError::Incomplete) => assert!(std::str::from_utf8(data).is_ok()),
        Err(SourceError::CsvTemplate) if matches!(format, Format::Csv) => assert!(std::str::from_utf8(data).is_ok()),
        Err(error) => panic!("unexpected file loader failure: {error:?}"),
    }
    assert_in_time(started);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every target's committed seeds, their first halves and a few edges.
    #[test]
    fn seed_corpus() {
        let (done, finished) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            // The nightly job runs every file in fuzz_targets/: each is a target here.
            let files = std::fs::read_dir(root.join("fuzz_targets"))
                .unwrap_or_else(|_| panic!("no fuzz targets"))
                .count();
            assert_eq!(files, TARGETS.len(), "a fuzz target missing from TARGETS");
            for (name, check, limit) in TARGETS {
                assert!(
                    root.join(format!("fuzz_targets/{name}.rs")).is_file(),
                    "{name} has no target"
                );
                let mut seeds = Vec::new();
                for entry in std::fs::read_dir(root.join("corpus").join(name))
                    .unwrap_or_else(|_| panic!("{name} has no seed corpus"))
                {
                    seeds.push(entry.unwrap_or_else(|_| panic!("seed entry unavailable")).path());
                }
                seeds.sort();
                assert!(!seeds.is_empty(), "{name}'s seed corpus must not be empty");
                for seed in seeds {
                    let data = std::fs::read(&seed).unwrap_or_else(|_| panic!("seed read failed"));
                    assert!(data.len() <= *limit, "{} exceeds the fuzz input limit", seed.display());
                    check(&data);
                    check(&data[..data.len() / 2]);
                }
                check(b"");
                check(b"\xff\xfe\0");
            }
            check_damaged_bibtex(b"@book{invalid,title={\xff}}");
            done.send(()).unwrap_or_else(|_| panic!("corpus watchdog unavailable"));
        });
        let outcome = finished.recv_timeout(INPUT_TIMEOUT * 12);
        assert_ne!(
            outcome,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout),
            "corpus exceeded one minute"
        );
        worker
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
        assert!(outcome.is_ok(), "corpus worker did not finish");
    }
}
