//! Capture the case-insensitive language-tag regression in #331.

use ibis_core::bibtex::load_str;

#[test]
fn english_language_tag_case_does_not_change_title_conversion() {
    let loaded = load_str(
        "synthetic.bib",
        "@book{a,title={The Second Title},langid={EN-US}}\n@book{b,title={The Second Title},langid={en-US}}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(loaded.references.len(), 2);
    assert_eq!(loaded.references[1].title_plain().as_deref(), Some("The second title"));
    assert_eq!(loaded.references[0].title_plain().as_deref(), Some("The second title"));
}

#[test]
fn mixed_case_primary_tags_keep_language_spelling_and_non_english_titles() {
    for (tag, expected) in [
        ("EN", "The second title"),
        ("En-Latn-US", "The second title"),
        ("eN-gB", "The second title"),
        ("DE-de", "The Second Title"),
        ("enm", "The Second Title"),
    ] {
        let loaded = load_str(
            "synthetic.bib",
            &format!("@book{{a,title={{The Second Title}},language={{{tag}}}}}"),
        );
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.references.len(), 1);
        assert_eq!(loaded.references[0].title_plain().as_deref(), Some(expected), "{tag}");
        assert_eq!(loaded.references[0].language.as_deref(), Some(tag));
    }
}
