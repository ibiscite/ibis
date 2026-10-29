//! BibTeX loader oracle shared by libFuzzer and the ordinary seed-corpus test.

use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ibis_core::contract::{CitationKeySource, LibraryObject, Snapshot};
use ibis_sources::{SourceError, load_file};

const MAX_INPUT_BYTES: usize = 64 * 1024;
const INPUT_TIMEOUT: Duration = Duration::from_secs(5);
static INSTALL_HOOK: Once = Once::new();
static PANICKED: AtomicBool = AtomicBool::new(false);

/// Exercise the public file loader with raw bytes, including invalid UTF-8.
/// Run serially: this standalone harness observes process-wide parser panics.
pub fn check_damaged_bibtex(data: &[u8]) {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    INSTALL_HOOK.call_once(|| {
        // Install after the production dispatcher so even a parser panic that
        // it contains is visible. Keep libFuzzer's abort hook for other panics.
        ibis_core::panic_guard::attempt(|| ()).unwrap_or_else(|_| panic!("panic guard setup failed"));
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            PANICKED.store(true, Ordering::Relaxed);
            previous(info);
        }));
    });

    let started = Instant::now();
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("temporary directory creation failed"));
    let path = directory.path().join("invented.bib");
    std::fs::write(&path, data).unwrap_or_else(|_| panic!("temporary input write failed"));
    let result = load_file(&path);
    assert!(!PANICKED.load(Ordering::Relaxed), "loader contained a parser panic");

    match result {
        Ok(loaded) => {
            assert!(std::str::from_utf8(data).is_ok(), "invalid UTF-8 was accepted");
            for item in loaded.snapshot.objects() {
                assert_eq!(item.object.id().library, loaded.library, "wrong library identity");
                if let LibraryObject::Record(record) = &item.object {
                    assert!(
                        record.citation_key().is_ok_and(|key| key.is_some()),
                        "invalid BibTeX citation key"
                    );
                    assert!(
                        record
                            .citation_keys
                            .iter()
                            .all(|key| key.source == CitationKeySource::Bibtex),
                        "wrong citation key provenance"
                    );
                    assert!(
                        record
                            .csl
                            .get("type")
                            .is_some_and(|value| value.as_str().is_some_and(|s| !s.is_empty()))
                    );
                    // Deserialize the actual wire representation too: record
                    // data must survive serialization without changing meaning.
                    let json = serde_json::to_vec(record).unwrap_or_else(|_| panic!("record serialization failed"));
                    let round_trip: ibis_core::contract::Record =
                        serde_json::from_slice(&json).unwrap_or_else(|_| panic!("invalid record wire data"));
                    assert_eq!(&round_trip, record);
                }
            }
            let objects = loaded
                .snapshot
                .objects()
                .iter()
                .map(|item| item.object.clone())
                .collect();
            let rebuilt = Snapshot::new(objects).unwrap_or_else(|_| panic!("invalid snapshot objects"));
            assert_eq!(rebuilt, loaded.snapshot, "snapshot validation or fingerprints changed");
        }
        Err(SourceError::Unreadable) => assert!(std::str::from_utf8(data).is_err()),
        // Rejecting malformed input is allowed; publishing invalid records is
        // not. Other errors indicate a broken harness or an unexpected I/O path.
        Err(SourceError::InvalidData) => assert!(std::str::from_utf8(data).is_ok()),
        Err(_) => panic!("unexpected file loader failure"),
    }
    // libFuzzer's -timeout also interrupts a loader that never returns. The
    // corpus test below uses a watchdog so it can fail on that same regression.
    assert!(started.elapsed() < INPUT_TIMEOUT, "input exceeded five seconds");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_corpus() {
        let (done, finished) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/damaged_bibtex");
            let mut seeds = Vec::new();
            for entry in std::fs::read_dir(corpus).unwrap_or_else(|_| panic!("seed corpus unavailable")) {
                let path = entry.unwrap_or_else(|_| panic!("seed entry unavailable")).path();
                if path.extension().is_some_and(|extension| extension == "bib") {
                    seeds.push(path);
                }
            }
            seeds.sort();
            assert!(!seeds.is_empty(), "seed corpus must not be empty");
            for seed in seeds {
                let data = std::fs::read(seed).unwrap_or_else(|_| panic!("seed read failed"));
                assert!(data.len() <= MAX_INPUT_BYTES, "seed exceeds fuzz input limit");
                check_damaged_bibtex(&data);
            }
            check_damaged_bibtex(b"");
            check_damaged_bibtex(b"@book{invalid,title={\xff}}");
            done.send(()).unwrap_or_else(|_| panic!("corpus watchdog unavailable"));
        });
        let outcome = finished.recv_timeout(INPUT_TIMEOUT);
        assert_ne!(
            outcome,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout),
            "corpus exceeded five seconds"
        );
        worker
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
        assert!(outcome.is_ok(), "corpus worker did not finish");
    }
}
