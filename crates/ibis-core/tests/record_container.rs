//! The container shown under a result: journal, book or proceedings, else publisher.

use ibis_core::bibtex::load_str;

fn container(bib: &str) -> Option<String> {
    let loaded = load_str("t", bib);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    loaded.references[0].container_plain()
}

#[test]
fn a_journal_article_shows_its_journal_without_markup() {
    let bib = r"@article{a, title={T}, journal={Journal of \emph{Synthetic} Studies}, publisher={Press}}";
    assert_eq!(container(bib).as_deref(), Some("Journal of Synthetic Studies"));
}

#[test]
fn a_chapter_shows_its_book_title() {
    let bib = "@incollection{a, title={T}, booktitle={Collected Synthetic Essays}, publisher={Press}}";
    assert_eq!(container(bib).as_deref(), Some("Collected Synthetic Essays"));
}

#[test]
fn a_book_without_a_container_shows_its_publisher() {
    assert_eq!(
        container("@book{a, title={T}, publisher={Example Press}}").as_deref(),
        Some("Example Press")
    );
    assert_eq!(
        container("@phdthesis{a, title={T}, school={Example University}}").as_deref(),
        Some("Example University")
    );
}

#[test]
fn blank_or_missing_containers_show_nothing() {
    assert_eq!(container("@misc{a, title={T}}"), None);
    let mut reference = load_str("t", "@misc{a, title={T}}").references.remove(0);
    reference.container_title = Some(" <i> </i> ".into());
    reference.variables.insert("publisher".into(), "  ".into());
    assert_eq!(reference.container_plain(), None);
    reference
        .variables
        .insert("publisher".into(), " Fallback  Press ".into());
    assert_eq!(reference.container_plain().as_deref(), Some("Fallback  Press"));
}
