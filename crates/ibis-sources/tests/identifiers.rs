//! Search identifiers found in every source field, never printed as CSL.
#![cfg(test)]

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
fn extra_identifiers_require_explicit_valid_labels_and_deduplicate() {
    let item: ZoteroItem = serde_json::from_value(json!({
        "key": "SYN00001", "itemType": "book", "title": "Synthetic identifiers",
        "extra": "PMID: 000000555\npmid: 555\nNot PMID: 999\nPMID: 000\nPMID: 1234567890\nPMID: invalid\narXiv: 2101.00001v2"
    })).unwrap();
    let record = zotero::map_item(LibraryId::digest(b"invented"), &item).unwrap().record;
    assert_eq!(
        of_record(&record),
        [
            Identifier::Doi("10.48550/arxiv.2101.00001".into()),
            Identifier::Arxiv("2101.00001".into()),
            Identifier::Pmid("555".into())
        ]
    );
}
