//! BibTeX 0.99d and Pandoc preserve values read before a redefinition (#349).

use ibis_core::bibtex::load_str;

#[test]
fn string_redefinitions_do_not_change_earlier_records() {
    let loaded = load_str(
        "synthetic.bib",
        "@string{j = \"First\"}\n@article{a,title={A},journal=j}\n@string{j = \"Second\"}\n@article{b,title={B},journal=j}\n",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(loaded.references.len(), 2);
    assert_eq!(loaded.references[0].container_title.as_deref(), Some("First"));
    assert_eq!(loaded.references[1].container_title.as_deref(), Some("Second"));
}

#[test]
fn aliases_capture_versions_without_false_cycles_or_cache_reuse() {
    let loaded = load_str(
        "synthetic.bib",
        "@string{j={First}} @string{alias=j}\n\
         @article{a,journal=alias}\n\
         @string{j=alias # {Second}}\n\
         @article{b,journal=j}\n\
         @article{c,journal=alias}\n\
         @string{j={Third}} @article{d,journal=j}\n\
         @article{e,journal=alias}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let journals: Vec<_> = loaded.references.iter().map(|r| r.container_title.as_deref()).collect();
    assert_eq!(
        journals,
        [
            Some("First"),
            Some("FirstSecond"),
            Some("First"),
            Some("Third"),
            Some("First")
        ]
    );
}

#[test]
fn inherited_fields_use_the_parents_definition_epoch() {
    let loaded = load_str(
        "synthetic.bib",
        "@string{j={First}}\n\
         @article{a,journal=j,crossref={parent}}\n\
         @string{j={Second}}\n\
         @proceedings{parent,publisher=j}\n\
         @string{j={Third}}\n\
         @article{b,journal=j,crossref={parent}}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    for (key, journal) in [("a", "First"), ("b", "Third")] {
        let r = loaded
            .references
            .iter()
            .find(|r| r.id.native_key == key)
            .unwrap_or_else(|| panic!("missing {key}"));
        assert_eq!(r.container_title.as_deref(), Some(journal));
        assert_eq!(r.variables.get("publisher").map(String::as_str), Some("Second"));
    }
}

#[test]
fn captured_months_keep_field_context_after_redefinition() {
    let loaded = load_str(
        "synthetic.bib",
        "@string{season=mar}\n\
         @article{a,title=season,month=season,year=2024}\n\
         @string{mar={May}}\n\
         @article{b,title=season,month=season,year=2024}\n\
         @article{c,title=mar,month=mar,year=2024}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    for (r, (title, month)) in loaded.references.iter().zip([("March", 3), ("March", 3), ("May", 5)]) {
        assert_eq!(r.title.as_deref(), Some(title));
        assert_eq!(r.issued.and_then(|d| d.start.month), Some(month));
    }
}

#[test]
fn future_definitions_and_direct_self_redefinitions_still_warn() {
    let loaded = load_str(
        "synthetic.bib",
        "@string{alias=later # {Suffix}}\n\
         @string{later={Prefix}}\n\
         @article{a,journal=alias}\n\
         @string{j={First}}\n\
         @string{j=j # {Second}}\n\
         @article{b,journal=j}",
    );
    assert_eq!(loaded.references.len(), 2);
    assert_eq!(loaded.references[0].container_title.as_deref(), Some("Suffix"));
    assert_eq!(loaded.references[1].container_title.as_deref(), Some("Second"));
    assert_eq!(loaded.warnings.len(), 2);
    assert!(
        loaded
            .warnings
            .iter()
            .any(|w| w.key.as_deref() == Some("a") && w.message.contains("undefined abbreviation `later`"))
    );
    assert!(
        loaded
            .warnings
            .iter()
            .any(|w| w.key.as_deref() == Some("b") && w.message.contains("abbreviation `j` refers to itself"))
    );
}
