//! Repeated hostile abbreviations must not repeat exponential work per field.

use std::time::Instant;

#[path = "support/subprocess_watchdog.rs"]
mod subprocess_watchdog;

use ibis_core::{bibtex::load_str, model::WarningKind};

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
        let loaded = load_str("synthetic.bib", &text);
        eprintln!("synthetic load: {:?}", start.elapsed());
        assert_eq!(loaded.references.len(), 1_001);
        assert_eq!(loaded.warnings.len(), 2_000);
        assert!(loaded.warnings.iter().all(|w| {
            w.kind == WarningKind::Field && w.line.is_some() && w.key.is_some() && w.message.contains("too complex")
        }));
        assert!(loaded.references[..1_000].iter().all(|r| r.year() == Some(2024)));
        assert_eq!(loaded.references[1_000].title.as_deref(), Some("Retained"));
        return;
    }
    subprocess_watchdog::assert_finishes(
        "repeated_doubling_abbreviations_finish_before_the_deadline",
        CHILD,
        "synthetic load exceeded the 2 second deadline",
    );
}

#[test]
fn cached_abbreviations_keep_month_context_overrides_and_warning_locations() {
    let definitions = "@string{season=mar}\n@string{digits={٢٠٢٤}}\n\
                       @string{bad=missing # {Suffix}}\n@string{cycle=cycle # {End}}\n";
    for fields in [
        "month=season,title=season,year=digits,note=bad,publisher=cycle",
        "title=season,month=season,year=digits,note=bad,publisher=cycle",
    ] {
        let text = format!("{definitions}@article{{a,{fields}}}\n@article{{b,{fields}}}\n");
        let loaded = load_str("synthetic.bib", &text);
        assert_eq!(loaded.references.len(), 2);
        for reference in &loaded.references {
            assert_eq!(reference.title.as_deref(), Some("March"));
            assert_eq!(reference.year(), Some(2024));
            assert_eq!(reference.issued.and_then(|d| d.start.month), Some(3));
        }
        assert_eq!(loaded.warnings.len(), 4);
        for (key, line) in [("a", 5), ("b", 6)] {
            let warnings: Vec<_> = loaded
                .warnings
                .iter()
                .filter(|w| w.key.as_deref() == Some(key))
                .collect();
            assert_eq!(warnings.len(), 2);
            assert!(warnings.iter().all(|w| w.line == Some(line)));
            assert!(
                warnings
                    .iter()
                    .any(|w| w.message.contains("undefined abbreviation `missing`"))
            );
            assert!(
                warnings
                    .iter()
                    .any(|w| w.message.contains("abbreviation `cycle` refers to itself"))
            );
        }
        let overridden = load_str("synthetic.bib", &format!("@string{{mar={{May}}}}\n{text}"));
        assert!(
            overridden
                .references
                .iter()
                .all(|r| { r.title.as_deref() == Some("May") && r.issued.and_then(|d| d.start.month) == Some(5) })
        );
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
    let loaded = load_str("synthetic.bib", &text);
    assert_eq!(loaded.references.len(), 3);
    assert_eq!(loaded.warnings.len(), 2);
    for (key, message) in [("steps", "too complex"), ("bytes", "too large")] {
        let reference = loaded
            .references
            .iter()
            .find(|r| r.id.native_key == key)
            .unwrap_or_else(|| panic!("missing {key}"));
        assert!(reference.title.as_deref().is_none_or(str::is_empty));
        assert_eq!(reference.year(), Some(2024));
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| w.key.as_deref() == Some(key) && w.message.contains(message))
        );
    }
}

#[test]
fn caching_a_silent_parent_expansion_keeps_its_later_warning() {
    let loaded = load_str(
        "synthetic.bib",
        "@string{bad=missing}\n\
         @article{child,crossref={parent}}\n\
         @book{parent,title=bad}\n\
         @xdata{helper,note=bad}\n\
         @article{a,xdata={helper}}\n\
         @article{b,xdata={helper}}\n",
    );
    assert_eq!(loaded.references.len(), 4);
    assert_eq!(loaded.warnings.len(), 3);
    for (key, line) in [("parent", 3), ("a", 5), ("b", 6)] {
        let warning = loaded
            .warnings
            .iter()
            .find(|w| w.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing warning for {key}"));
        assert_eq!(warning.line, Some(line));
        assert!(warning.message.contains("undefined abbreviation `missing`"));
        assert_eq!(warning.message.contains("in inherited `helper`"), key != "parent");
    }
}
