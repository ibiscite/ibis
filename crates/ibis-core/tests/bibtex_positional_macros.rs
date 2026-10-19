//! BibTeX 0.99d and Pandoc preserve values read before a redefinition (#349).

use ibis_core::bibtex::load_str;

#[test]
#[ignore = "#349: string redefinitions retroactively change earlier records"]
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
