//! Capture distinct exact-case inheritance keys losing metadata (#344).

use ibis_core::bibtex::load_str;

#[test]
fn case_distinct_xdata_keys_both_supply_their_fields() {
    let loaded = load_str(
        "synthetic.bib",
        "@xdata{A,publisher={First}}\n@xdata{a,note={Second}}\n@book{b,xdata={A,a},title={T}}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(loaded.references.len(), 1);
    assert_eq!(
        loaded.references[0].variables.get("publisher").map(String::as_str),
        Some("First")
    );
    assert_eq!(
        loaded.references[0].variables.get("note").map(String::as_str),
        Some("Second")
    );
}

#[test]
fn exact_duplicate_parents_keep_last_occurrence_precedence() {
    for (parents, expected) in [("A,a", "Second"), ("a,A", "First"), ("A,a,A", "First")] {
        let loaded = load_str(
            "synthetic.bib",
            &format!(
                "@xdata{{A,publisher={{First}}}}\n@xdata{{a,publisher={{Second}}}}\n@book{{b,xdata={{{parents}}},title={{T}}}}"
            ),
        );
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.references.len(), 1);
        assert_eq!(
            loaded.references[0].variables.get("publisher").map(String::as_str),
            Some(expected),
            "{parents}"
        );
    }
}
