//! Work limits: hostile BibTeX is bounded, ordinary large libraries are never
//! truncated, and the file size boundary is exact. Synthetic data only.

#[path = "support/bib.rs"]
mod bib;
#[path = "support/subprocess_watchdog.rs"]
mod subprocess_watchdog;

use std::io::{Read, Write};
use std::time::Instant;

use bib::{find, keys, load, records, text, warned_at, year};
use ibis_sources::{MAX_LIBRARY_BYTES, SourceError, WarningCode};

// --- ordinary libraries load completely -------------------------------------------

fn assert_complete(text: &str, count: usize) {
    let start = Instant::now();
    let loaded = load(text);
    eprintln!(
        "ordinary library: bytes={}, references={}, warnings={}, elapsed={:?}",
        text.len(),
        records(&loaded).len(),
        loaded.warnings.len(),
        start.elapsed()
    );
    assert_eq!(records(&loaded).len(), count);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert!(find(&loaded, &format!("k{}", count - 1)).is_some());
}

#[test]
fn twenty_thousand_ordinary_ten_field_records_load_completely() {
    let fields = "title={Small synthetic record},author={Doe, Jane},journal={Sample Journal},year={2024},\
                  volume={12},number={3},pages={1--9},doi={10.0000/example},url={https://example.org/r},keywords={sample; test}";
    let text: String = (0..20_000).map(|i| format!("@article{{k{i},{fields}}}\n")).collect();
    assert_complete(&text, 20_000);
}

#[test]
fn large_libraries_load_in_linear_time() {
    let mut src = String::from("@string{j = \"Journal\"}\n@proceedings{conf, title={Conf}, year={2020}}\n");
    for i in 0..5_000 {
        src.push_str(&format!(
            "@inproceedings{{k{i}, title={{Paper {i}}}, crossref={{conf}}, booktitle = j}}\n"
        ));
    }
    let start = Instant::now();
    let r = load(&src);
    assert_eq!(records(&r).len(), 5_001);
    assert!(start.elapsed().as_secs() < 20, "took {:?}", start.elapsed());
}

// --- hostile work is bounded -----------------------------------------------------------

#[test]
fn repeated_doubling_abbreviations_finish_before_the_deadline() {
    const CHILD: &str = "IBIS_MACRO_CACHE_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let mut text = String::from("@string{m0={}}\n");
        for i in 1..=17 {
            text.push_str(&format!("@string{{m{i}=m{} # m{}}}\n", i - 1, i - 1));
        }
        for i in 0..1_000 {
            text.push_str(&format!("@article{{k{i},title=m17,note=m17,year=2024}}\n"));
        }
        text.push_str("@article{kept,title={Retained}}\n");
        let start = Instant::now();
        let loaded = load(&text);
        eprintln!("synthetic load: {:?}", start.elapsed());
        assert_eq!(records(&loaded).len(), 1_001);
        assert_eq!(loaded.warnings.len(), 2_000);
        assert!(
            loaded
                .warnings
                .iter()
                .all(|w| w.code == WarningCode::Field && w.line.is_some())
        );
        assert!((0..1_000).all(|i| year(&loaded, &format!("k{i}")) == Some(2024)));
        assert_eq!(text_of(&loaded, "kept"), Some("Retained"));
        return;
    }
    subprocess_watchdog::assert_finishes(
        "repeated_doubling_abbreviations_finish_before_the_deadline",
        CHILD,
        "synthetic load exceeded the 2 second deadline",
    );
}

fn text_of<'a>(loaded: &'a ibis_sources::LoadedLibrary, key: &str) -> Option<&'a str> {
    text(loaded, key, "title")
}

#[test]
fn exponential_macro_expansion_is_bounded() {
    let mut src = String::from("@string{m0 = \"xxxxxxxxxxxxxxxx\"}\n");
    for i in 1..=24 {
        src.push_str(&format!("@string{{m{i} = m{} # m{}}}\n", i - 1, i - 1));
    }
    src.push_str("@article{boom, title = m24}\n");
    let start = Instant::now();
    let r = load(&src);
    assert!(start.elapsed().as_secs() < 5, "took {:?}", start.elapsed());
    let title_len = text_of(&r, "boom").map_or(0, str::len);
    assert!(title_len < 1_000_000, "expanded to {title_len} bytes");
    // The oversized expansion is reported on the entry that used it.
    assert_eq!(warned_at(&r, WarningCode::Field, 26), 1, "{:?}", r.warnings);
}

#[test]
fn repeated_and_fanned_out_inheritance_is_bounded() {
    let mut src = String::from("@book{r,title={T},xdata={a}}\n@xdata{a,xdata={b,b,b,b,b,b,b,b}}\n");
    for (x, y) in [("b", "c"), ("c", "d"), ("d", "e"), ("e", "f"), ("f", "g"), ("g", "h")] {
        src.push_str(&format!("@xdata{{{x},xdata={{{y},{y},{y},{y},{y},{y},{y},{y}}}}}\n"));
    }
    src.push_str("@xdata{h,year=2024}\n");
    // Distinct parents fanning out at every level (8 x 8 x 8 paths).
    src.push_str("@book{wide,title={W},xdata={w0,w1,w2,w3,w4,w5,w6,w7}}\n");
    for i in 0..8 {
        src.push_str(&format!("@xdata{{w{i},xdata={{v0,v1,v2,v3,v4,v5,v6,v7}}}}\n"));
        src.push_str(&format!("@xdata{{v{i},xdata={{u0,u1,u2,u3,u4,u5,u6,u7}}}}\n"));
        src.push_str(&format!("@xdata{{u{i},publisher={{P}}}}\n"));
    }
    let start = Instant::now();
    let r = load(&src);
    assert!(start.elapsed().as_secs() < 5, "took {:?}", start.elapsed());
    assert_eq!(year(&r, "r"), Some(2024));
    assert_eq!(text(&r, "wide", "publisher"), Some("P"));
}

// --- abbreviation caching keeps per-use meaning and warnings ------------------------

// --- size boundary -----------------------------------------------------------------------

#[test]
#[allow(clippy::disallowed_methods)]
fn exactly_sixty_four_mib_loads_and_one_more_byte_is_rejected() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("参考文献 synthetic.bib");
    let first = b"@misc{first,title={Synthetic first}}\n";
    let last = b"@misc{last,title={Synthetic last}}\n";
    let padding = MAX_LIBRARY_BYTES - (first.len() + last.len()) as u64;
    {
        let mut file = std::fs::File::create(&path).unwrap_or_else(|e| panic!("{e}"));
        file.write_all(first).unwrap_or_else(|e| panic!("{e}"));
        std::io::copy(&mut std::io::repeat(b' ').take(padding), &mut file).unwrap_or_else(|e| panic!("{e}"));
        file.write_all(last).unwrap_or_else(|e| panic!("{e}"));
    }
    assert_eq!(
        std::fs::metadata(&path).unwrap_or_else(|e| panic!("{e}")).len(),
        64 * 1024 * 1024
    );
    for reload in [false, true] {
        let loaded = if reload {
            ibis_sources::reload_file(&path)
        } else {
            ibis_sources::load_file(&path)
        }
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(keys(&loaded), ["first", "last"]);
        assert!(loaded.warnings.is_empty());
    }
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap_or_else(|e| panic!("{e}"))
        .write_all(b" ")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(ibis_sources::load_file(&path).err(), Some(SourceError::TooLarge));
    assert_eq!(ibis_sources::reload_file(&path).err(), Some(SourceError::TooLarge));
}
