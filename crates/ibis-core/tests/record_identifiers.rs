//! Search identifiers stay out of printed CSL fields and citation fingerprints.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use serde_json::json;

fn seed() -> GoldenCase {
    serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap()
}
fn changed(case: &GoldenCase, change: impl FnOnce(&mut Record)) -> Snapshot {
    let mut objects: Vec<_> = case.library.objects().iter().map(|item| item.object.clone()).collect();
    let LibraryObject::Record(record) = &mut objects[0] else {
        panic!("synthetic seed");
    };
    change(record);
    Snapshot::new(objects).unwrap()
}

#[test]
fn search_only_identifiers_round_trip_without_invalidating_citation_intent() {
    use ibis_core::identifier::Identifier;
    let case = seed();
    let snapshot = changed(&case, |record| {
        record.identifiers = vec![Identifier::Arxiv("2101.00001".into())]
    });
    assert_eq!(
        snapshot.objects()[0].fingerprints.cite,
        case.library.objects()[0].fingerprints.cite
    );
    assert_ne!(
        snapshot.objects()[0].fingerprints.full,
        case.library.objects()[0].fingerprints.full
    );
    let wire = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(
        wire["objects"][0]["object"]["data"]["identifiers"],
        json!([{"kind":"arxiv","value":"2101.00001"}])
    );
    assert_eq!(serde_json::from_value::<Snapshot>(wire).unwrap(), snapshot);
    let LibraryObject::Record(record) = &snapshot.objects()[0].object else {
        panic!("seed");
    };
    assert_eq!(
        ibis_core::identifier::of_record(record),
        [
            Identifier::Doi("10.48550/arxiv.2101.00001".into()),
            Identifier::Arxiv("2101.00001".into())
        ]
    );
    for id in [
        Identifier::Doi("10.1000/synthetic".into()),
        Identifier::Isbn("9780306406157".into()),
        Identifier::Arxiv("2101.00001".into()),
        Identifier::Pmid("12345".into()),
    ] {
        assert_eq!(
            serde_json::from_value::<Identifier>(serde_json::to_value(&id).unwrap()).unwrap(),
            id
        );
    }
}
