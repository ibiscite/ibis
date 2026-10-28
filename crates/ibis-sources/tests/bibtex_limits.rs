//! Work limits: hostile BibTeX is bounded, ordinary large libraries are never
//! truncated, and the file size boundary is exact. Synthetic data only.

#[path = "support/bib.rs"]
mod bib;
#[path = "support/subprocess_watchdog.rs"]
mod subprocess_watchdog;

use std::io::{Read, Write};
use std::time::Instant;

use bib::{csl, find, keys, load, month, records, text, warned_at, with_code, year};
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
fn two_hundred_long_notes_load_completely() {
    let note = "x".repeat(65_535);
    let text: String = (0..200)
        .map(|i| format!("@article{{k{i},title={{Paper {i}}},year=2024,note={{{note}}}}}\n"))
        .collect();
    assert_complete(&text, 200);
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

#[test]
fn block_splitting_scales_linearly() {
    let build = |n: usize| {
        (0..n)
            .map(|i| format!("@misc{{k{i}, title={{T{i}}}}}\n"))
            .collect::<String>()
    };
    let small_src = build(2_000);
    let large_src = build(20_000);
    let time = |src: &str, count: usize, repeats: usize| {
        let start = Instant::now();
        for _ in 0..repeats {
            let result = load(src);
            assert_eq!(records(&result).len(), count);
            assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        }
        start.elapsed()
    };
    // Equal total records per batch give both sizes a comparable scheduling
    // window. Adjacent, alternating batches avoid timing all small loads before
    // all large loads while competing builds can change the machine's load.
    let mut samples = (0..3)
        .map(|round| {
            let (small, large) = if round % 2 == 0 {
                let small = time(&small_src, 2_000, 10);
                (small, time(&large_src, 20_000, 1))
            } else {
                let large = time(&large_src, 20_000, 1);
                (time(&small_src, 2_000, 10), large)
            };
            (large.as_secs_f64() / small.as_secs_f64(), small, large)
        })
        .collect::<Vec<_>>();
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (ratio, small, large) = samples[1];
    // 10x input must take <25x time, so one large load must take <2.5x ten
    // small loads. Quadratic growth is ~10x per equal-work batch. The median
    // tolerates one interrupted batch pair.
    assert!(ratio < 2.5, "10 × 2k: {small:?}, 20k: {large:?}; samples: {samples:?}");
}

// --- hostile work is bounded -----------------------------------------------------------

#[test]
fn distinct_aliases_share_one_load_budget() {
    const CHILD: &str = "IBIS_LOAD_BUDGET_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let mut text = String::from("@article{kept,title={Retained}}\n@string{m0={}}\n");
        for i in 1..=17 {
            text.push_str(&format!("@string{{m{i}=m{} # m{}}}\n", i - 1, i - 1));
        }
        for i in 0..1_000 {
            text.push_str(&format!(
                "@string{{alias{i}=m17}}\n@article{{k{i},title=alias{i},year=2024}}\n"
            ));
        }
        text.push_str("@article{suffix,title={Skipped}}\n");
        let start = Instant::now();
        let loaded = load(&text);
        eprintln!("synthetic load: {:?}", start.elapsed());
        assert!(find(&loaded, "kept").is_some());
        assert!(records(&loaded).len() < 1_001);
        assert!(find(&loaded, "suffix").is_none());
        let limits = with_code(&loaded, WarningCode::WorkLimit);
        assert_eq!(limits.len(), 1, "{:?}", loaded.warnings);
        assert!(limits[0].line.is_some());
        return;
    }
    subprocess_watchdog::assert_finishes(
        "distinct_aliases_share_one_load_budget",
        CHILD,
        "synthetic load exceeded the 2 second deadline",
    );
}

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
fn long_author_whitespace_runs_finish_before_the_deadline() {
    const CHILD: &str = "IBIS_AUTHOR_WHITESPACE_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let start = Instant::now();
        for bytes in [16 * 1024, 32 * 1024, 64 * 1024] {
            for author in [
                format!("A{}B", " ".repeat(bytes - 2)),
                format!("A B{}", " ".repeat(bytes - 3)),
            ] {
                let loaded = load(&format!("@article{{k,title={{Synthetic}},author={{{author}}}}}\n"));
                assert_eq!(keys(&loaded), ["k"]);
                assert!(loaded.warnings.is_empty());
                let authors = csl(&loaded, "k")["author"].as_array().cloned().unwrap_or_default();
                assert_eq!(authors.len(), 1);
                assert_eq!(authors[0]["family"], "B");
                assert_eq!(authors[0]["given"], "A");
            }
        }
        eprintln!("synthetic author load: {:?}", start.elapsed());
        return;
    }
    subprocess_watchdog::assert_finishes(
        "long_author_whitespace_runs_finish_before_the_deadline",
        CHILD,
        "synthetic author load exceeded the 2 second deadline",
    );
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

#[test]
fn cached_abbreviations_keep_month_context_overrides_and_warning_locations() {
    let definitions = "@string{season=mar}\n@string{digits={٢٠٢٤}}\n\
                       @string{bad=missing # {Suffix}}\n@string{cycle=cycle # {End}}\n";
    for fields in [
        "month=season,title=season,year=digits,note=bad,publisher=cycle",
        "title=season,month=season,year=digits,note=bad,publisher=cycle",
    ] {
        let text = format!("{definitions}@article{{a,{fields}}}\n@article{{b,{fields}}}\n");
        let loaded = load(&text);
        assert_eq!(keys(&loaded), ["a", "b"]);
        for key in ["a", "b"] {
            assert_eq!(text_of(&loaded, key), Some("March"));
            assert_eq!(year(&loaded, key), Some(2024));
            assert_eq!(month(&loaded, key), Some(3));
        }
        assert_eq!(loaded.warnings.len(), 4, "{:?}", loaded.warnings);
        // The undefined `missing` and the self-referencing `cycle` are each
        // reported on both entries that use them.
        for line in [5, 6] {
            assert_eq!(warned_at(&loaded, WarningCode::Field, line), 2, "line {line}");
        }
        let overridden = load(&format!("@string{{mar={{May}}}}\n{text}"));
        for key in ["a", "b"] {
            assert_eq!(text_of(&overridden, key), Some("May"));
            assert_eq!(month(&overridden, key), Some(5));
        }
    }
}

#[test]
fn cached_parts_still_share_the_original_field_quotas() {
    // Two individually valid empty expansions together exceed the step quota.
    let mut text = String::from("@string{m0={}}\n");
    for i in 1..=15 {
        text.push_str(&format!("@string{{m{i}=m{} # m{}}}\n", i - 1, i - 1));
    }
    // Seed both caches before using them as a later part in a field.
    text.push_str("@string{large={");
    text.push_str(&"x".repeat(64 * 1024));
    text.push_str("}}\n@article{seed,title=m15,note=large}\n");
    text.push_str("@article{steps,title=m15 # m15,year=2024}\n");
    text.push_str("@article{bytes,title={prefix} # large,year=2024}\n");
    let loaded = load(&text);
    assert_eq!(keys(&loaded), ["bytes", "seed", "steps"]);
    assert_eq!(loaded.warnings.len(), 2, "{:?}", loaded.warnings);
    // `steps` (line 19) is too complex and `bytes` (line 20) too large.
    for (key, line) in [("steps", 19), ("bytes", 20)] {
        assert!(text_of(&loaded, key).is_none_or(str::is_empty), "{key}");
        assert_eq!(year(&loaded, key), Some(2024));
        assert_eq!(warned_at(&loaded, WarningCode::Field, line), 1, "{key}");
    }
}

#[test]
fn caching_a_silent_parent_expansion_keeps_its_later_warning() {
    let loaded = load(
        "@string{bad=missing}\n\
         @article{child,crossref={parent}}\n\
         @book{parent,title=bad}\n\
         @xdata{helper,note=bad}\n\
         @article{a,xdata={helper}}\n\
         @article{b,xdata={helper}}\n",
    );
    assert_eq!(records(&loaded).len(), 4);
    assert_eq!(loaded.warnings.len(), 3, "{:?}", loaded.warnings);
    for line in [3, 5, 6] {
        assert_eq!(warned_at(&loaded, WarningCode::Field, line), 1, "line {line}");
    }
}

// --- size boundary -----------------------------------------------------------------------

#[test]
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
