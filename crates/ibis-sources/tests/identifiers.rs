//! Search identifiers found in every source field, never printed as CSL.
#![allow(clippy::unwrap_used)]

use ibis_core::contract::{LibraryId, LibraryObject, Record};
use ibis_core::identifier::{Identifier, of_record};
use ibis_sources::zotero::{self, ZoteroItem};
use serde_json::json;

/// Records of a synthetic `.bib` text, by citation key.
fn bib(text: &str) -> Vec<Record> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.bib");
    std::fs::write(&path, text).unwrap();
    let loaded = ibis_sources::load_file(&path).unwrap();
    let mut records: Vec<Record> = loaded
        .snapshot
        .objects()
        .iter()
        .filter_map(|item| match &item.object {
            LibraryObject::Record(record) => Some(record.clone()),
            _ => None,
        })
        .collect();
    records.sort_by_key(|record| record.citation_key().unwrap().map(str::to_string));
    records
}

#[test]
fn records_expose_identifiers_from_every_field() {
    let records = bib("@book{a, title={T}, doi={https://doi.org/10.48550/arXiv.2101.00001},
          url={https://pubmed.ncbi.nlm.nih.gov/0012345/}, isbn={0-306-40615-2, 9780306406157},
          number={arXiv:2101.00001}}");
    let ids = of_record(&records[0]);
    for id in [
        Identifier::Doi("10.48550/arxiv.2101.00001".into()),
        Identifier::Isbn("9780306406157".into()),
        Identifier::Arxiv("2101.00001".into()),
        Identifier::Pmid("12345".into()),
    ] {
        assert!(ids.contains(&id), "{id:?} in {ids:?}");
    }
}

#[test]
fn records_from_bibtex_eprint_pmid_and_spaced_isbn_fields() {
    let records = bib("@misc{a, title={A}, eprint={hep-th/9711200}, archivePrefix={arXiv}}
         @article{b, title={B}, journal={J}, eprint={1706.03762v5}, eprinttype={arxiv}, pmid={12345678}, pmcid={PMC7654321}}
         @book{c, title={C}, isbn={978 0 306 40615 7}}
         @article{d, title={D}, journal={J}, eprint={987}, eprinttype={pubmed}}
         @misc{e, title={E}, eprint={arXiv:1706.03763}, archivePrefix={arXiv}}");
    let ids: Vec<_> = records.iter().map(of_record).collect();
    assert!(ids[0].contains(&Identifier::Arxiv("hep-th/9711200".into())));
    assert!(ids[1].contains(&Identifier::Arxiv("1706.03762".into())));
    assert!(ids[1].contains(&Identifier::Doi("10.48550/arxiv.1706.03762".into())));
    assert!(ids[1].contains(&Identifier::Pmid("12345678".into())));
    // BibTeX `pmid` and `pmcid` become the CSL variables Zotero uses.
    assert_eq!(records[1].csl["PMID"], "12345678");
    assert_eq!(records[1].csl["PMCID"], "PMC7654321");
    assert_eq!(records[3].csl["PMID"], "987");
    assert_eq!(ids[2], [Identifier::Isbn("9780306406157".into())]);
    assert_eq!(ids[3], [Identifier::Pmid("987".into())]);
    assert!(ids[4].contains(&Identifier::Arxiv("1706.03763".into())));
}

#[test]
fn an_arxiv_eprint_is_searchable_but_never_printed() {
    let records = bib(
        "@article{a, title={A}, journal={J}, number={4}, eprint={2101.00001}, eprinttype={arxiv}}
         @techreport{b, title={B}, institution={I}, number={TR-7}, eprint={2101.00002}, archivePrefix={arXiv}}",
    );
    assert!(of_record(&records[0]).contains(&Identifier::Arxiv("2101.00001".into())));
    assert!(of_record(&records[1]).contains(&Identifier::Arxiv("2101.00002".into())));
    // Citation processors see the same item as without the eprint.
    let csl = &records[0].csl;
    assert_eq!(csl.get("number"), None);
    assert_eq!(csl["issue"], "4");
    assert_eq!(records[1].csl["number"], "TR-7");
    assert!(!serde_json::to_string(csl).unwrap().contains("2101.00001"));
}

#[test]
fn records_from_zotero_carry_pmid_isbn_and_arxiv() {
    let item: ZoteroItem = serde_json::from_value(json!({
        "key": "SYN00001", "itemType": "journalArticle", "title": "T",
        "ISBN": "0-306-40615-2", "extra": "PMID: 555\narXiv: 2101.00001"
    }))
    .unwrap();
    let record = zotero::map_item(LibraryId::zotero(b"invented"), &item).unwrap().record;
    let ids = of_record(&record);
    assert!(ids.contains(&Identifier::Pmid("555".into())), "{ids:?}");
    assert!(ids.contains(&Identifier::Isbn("9780306406157".into())), "{ids:?}");
    assert!(ids.contains(&Identifier::Arxiv("2101.00001".into())), "{ids:?}");
}
