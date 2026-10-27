//! Invented data only. No Zotero process or library is used.
#![allow(clippy::unwrap_used)]
use ibis_sources::zotero;

use ibis_core::contract::LibraryId;
use serde_json::json;

#[test]
fn neutral_book_maps_without_zotero_export() {
    let item = serde_json::from_value(json!({
        "key": "SYN00001", "itemType": "book", "title": "Invented book",
        "creators": [{"creatorType": "author", "firstName": "Example", "lastName": "Synthetic"}],
        "date": "2026-10-25", "citationKey": "Synthetic2026"
    }))
    .unwrap();
    let mapped = zotero::map_item(LibraryId::zotero(b"invented library"), &item).unwrap();
    assert_eq!(mapped.record.csl["type"], "book");
    assert_eq!(mapped.record.csl["title"], "Invented book");
    assert_eq!(
        mapped.record.csl["author"],
        json!([{"family": "Synthetic", "given": "Example"}])
    );
    assert_eq!(mapped.record.csl["issued"], json!({"date-parts": [[2026, 10, 25]]}));
    assert_eq!(mapped.record.citation_key().unwrap(), Some("Synthetic2026"));
}

#[test]
fn differential_against_pinned_zotero_covers_every_bibliographic_type() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../../fixtures/zotero/mapping/cases.json")).unwrap();
    let schema: serde_json::Value = serde_json::from_str(include_str!("../vendor/zotero/schema.json")).unwrap();
    let mut covered = std::collections::BTreeSet::new();
    let mut differences = Vec::new();
    for case in &cases {
        let item: zotero::ZoteroItem = serde_json::from_value(case["item"].clone()).unwrap();
        let mapped = zotero::map_item(LibraryId::zotero(b"invented library"), &item).unwrap();
        let actual = serde_json::to_value(&mapped.record.csl).unwrap();
        if actual != case["expected"] {
            differences.push(format!(
                "{}: actual {} expected {}",
                case["name"], actual, case["expected"]
            ));
        }
        covered.insert(item.item_type);
    }
    let wanted: std::collections::BTreeSet<_> = schema["itemTypes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|kind| kind["itemType"].as_str())
        .filter(|kind| !["attachment", "note", "annotation"].contains(kind))
        .map(str::to_owned)
        .collect();
    assert_eq!(covered, wanted);
    assert!(
        differences.is_empty(),
        "{} mismatches:\n{}",
        differences.len(),
        differences.join("\n")
    );
}

fn item(fields: serde_json::Value) -> zotero::ZoteroItem {
    let mut item = json!({"key": "SYN00001", "itemType": "book"});
    item.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    serde_json::from_value(item).unwrap()
}
fn mapped(fields: serde_json::Value) -> zotero::MappedItem {
    zotero::map_item(LibraryId::zotero(b"invented library"), &item(fields)).unwrap()
}

#[test]
fn all_key_sources_survive_with_provenance_and_conflicts_are_never_hidden() {
    use ibis_core::contract::{CitationKeySource, ModelError};
    let agreeing = mapped(
        json!({"citationKey": " Synthetic2026 ", "extra": "citation key: Synthetic2026", "better_bibtex_keys": ["Synthetic2026"]}),
    );
    assert_eq!(agreeing.record.citation_key().unwrap(), Some("Synthetic2026"));
    assert_eq!(
        agreeing
            .record
            .citation_keys
            .iter()
            .map(|key| key.source)
            .collect::<Vec<_>>(),
        vec![
            CitationKeySource::Zotero,
            CitationKeySource::Extra,
            CitationKeySource::BetterBibtex
        ]
    );
    for fields in [
        json!({"citationKey": "Synthetic2026", "extra": "Citation Key: Different2026"}),
        json!({"extra": "Citation Key: Synthetic2026\nCitation Key: Different2026"}),
        json!({"citationKey": "Synthetic2026", "better_bibtex_keys": ["Different2026"]}),
        json!({"better_bibtex_keys": ["Synthetic2026", "Different2026"]}),
    ] {
        assert_eq!(
            mapped(fields).record.citation_key(),
            Err(ModelError::ConflictingCitationKeys)
        );
    }
    assert_eq!(mapped(json!({})).record.citation_key(), Ok(None));
    let invalid = mapped(json!({"citationKey": "bad key", "extra": "Citation Key: Safe2026"}));
    assert!(invalid.warnings.contains(&zotero::MappingWarning::InvalidCitationKey));
    assert_eq!(invalid.record.citation_key(), Err(ModelError::ConflictingCitationKeys));
}

#[test]
fn aliases_are_explicit_deduplicated_and_do_not_replace_native_identity() {
    let result = mapped(
        json!({"citationKey": "Synthetic2026", "extra": "tex.ids= Earlier2020, Earlier2021, Earlier2020\nTeX.IDS: 日本語, invalid alias"}),
    );
    assert_eq!(result.record.aliases, ["Earlier2020", "Earlier2021", "日本語"]);
    assert_eq!(result.record.id.key, "SYN00001");
    assert_eq!(result.record.citation_key(), Ok(Some("Synthetic2026")));
    assert!(result.warnings.contains(&zotero::MappingWarning::InvalidAlias));
}

#[test]
fn mapped_keys_use_existing_refusals_for_missing_conflicting_and_ambiguous_owners() {
    use ibis_core::contract::{CitationError, CitationRequest, LibraryObject, Snapshot};
    let case: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/cases/09-conflicting-key.json")).unwrap();
    let mut request: CitationRequest = serde_json::from_value(case["request"].clone()).unwrap();
    for (fields, other, expected) in [
        (json!({}), None, CitationError::MissingKey),
        (
            json!({"citationKey": "Synthetic2026", "extra": "Citation Key: Different2026"}),
            None,
            CitationError::ConflictingKey,
        ),
        (
            json!({"citationKey": "Synthetic2026"}),
            Some(json!({"citationKey": "Synthetic2026"})),
            CitationError::AmbiguousKey,
        ),
        (
            json!({"citationKey": "Synthetic2026"}),
            Some(json!({"citationKey": "Other2026", "extra": "tex.ids= Synthetic2026"})),
            CitationError::AmbiguousKey,
        ),
    ] {
        let record = mapped(fields).record;
        request.items[0].item = record.id.clone();
        request.items[0].expected_cite = None;
        let mut objects = vec![LibraryObject::Record(record)];
        if let Some(fields) = other {
            let mut record = mapped(fields).record;
            record.id.key = "SYN00002".into();
            objects.push(LibraryObject::Record(record));
        }
        let result = ibis_core::syntax::cite(&Snapshot::new(objects).unwrap(), &request).unwrap();
        assert_eq!(
            serde_json::to_value(result.outcomes[0]).unwrap()["error"],
            serde_json::to_value(expected).unwrap()
        );
        assert!(result.text.is_empty());
    }
}

#[test]
fn malformed_oversized_and_nonbibliographic_inputs_are_bounded_and_private() {
    use zotero::{MappingError, MappingWarning};
    for item_type in ["attachment", "note", "annotation", "unrecognized"] {
        assert_eq!(
            zotero::map_item(LibraryId::zotero(b"invented"), &item(json!({"itemType": item_type}))),
            Err(MappingError::UnsupportedType)
        );
    }
    for key in ["", "bad key", "bad\nkey"] {
        assert_eq!(
            zotero::map_item(LibraryId::zotero(b"invented"), &item(json!({"key": key}))),
            Err(MappingError::InvalidIdentity)
        );
    }
    for fields in [
        json!({"title": "x".repeat(256 * 1024 + 1)}),
        json!({"creators": vec![json!({"creatorType":"author"}); 4097]}),
    ] {
        assert_eq!(
            zotero::map_item(LibraryId::zotero(b"invented"), &item(fields)),
            Err(MappingError::InputLimit)
        );
    }
    for malformed in [
        json!({"title": {"not": "text"}}),
        json!({"creators": [{"creatorType": "author", "name": []}]}),
        json!({"title": 3}),
    ] {
        let mut value = json!({"key": "SYN00001", "itemType": "book"});
        value
            .as_object_mut()
            .unwrap()
            .extend(malformed.as_object().unwrap().clone());
        assert!(serde_json::from_value::<zotero::ZoteroItem>(value).is_err());
    }
    let data = mapped(
        json!({"SYNTHETIC_PRIVATE_FIELD": "invented private marker", "creators": [{"creatorType": "unknown", "name": "Synthetic private creator"}], "title": "日本語 <script>invented</script>"}),
    );
    assert!(data.warnings.contains(&MappingWarning::UnknownField));
    assert!(data.warnings.contains(&MappingWarning::SkippedCreator));
    let public = serde_json::to_string(&data.record).unwrap();
    assert!(!public.contains("PRIVATE_FIELD"));
    assert!(!public.contains("private marker"));
    assert_eq!(data.record.csl["title"], "日本語 <script>invented</script>");
    assert!(data.record.raw.is_some());
    for warning in data.warnings {
        assert!(!format!("{warning:?}").contains("Synthetic"));
    }
    for url in [
        "file:///invented/private.pdf",
        "/invented/private.pdf",
        "../invented.pdf",
        "C:\\invented\\private.pdf",
        "\\\\invented\\private.pdf",
    ] {
        let data = mapped(json!({"url": url, "extra": format!("URL: {url}")}));
        assert!(data.warnings.contains(&MappingWarning::PrivateUrl));
        assert!(!data.record.csl.contains_key("URL"));
        assert!(!serde_json::to_string(&data.record).unwrap().contains("private.pdf"));
    }
}

#[test]
fn aggregate_work_limits_and_invalid_keys_do_not_hide_other_sources() {
    use zotero::MappingError;
    let mut source = item(json!({}));
    source.fields = (0..257)
        .map(|index| (format!("invented-{index}"), String::new()))
        .collect();
    assert_eq!(
        zotero::map_item(LibraryId::zotero(b"invented"), &source),
        Err(MappingError::InputLimit)
    );
    let many_keys = (0..4097)
        .map(|index| format!("Citation Key: Invented{index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let many_aliases = format!(
        "tex.ids= {}",
        (0..4097)
            .map(|index| format!("Alias{index}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    for extra in [many_keys, many_aliases] {
        assert_eq!(
            zotero::map_item(LibraryId::zotero(b"invented"), &item(json!({"extra": extra}))),
            Err(MappingError::InputLimit)
        );
    }
    let mut source = item(json!({}));
    source.fields = (0..9)
        .map(|index| (format!("invented-{index}"), "x".repeat(256 * 1024)))
        .collect();
    assert_eq!(
        zotero::map_item(LibraryId::zotero(b"invented"), &source),
        Err(MappingError::InputLimit)
    );
    for citation_key in ["bad key", "bad\0key"] {
        let result = mapped(json!({"citationKey": citation_key}));
        assert!(result.record.citation_key().is_err());
    }
}

proptest::proptest! {
    #[test]
    fn synthetic_unicode_fields_creators_and_extra_never_panic(
        title in ".{0,160}", date in ".{0,160}", extra in ".{0,200}",
        first_name in ".{0,100}", last_name in ".{0,100}"
    ) {
        use ibis_core::contract::{LibraryObject, Snapshot};
        let result = mapped(json!({
            "title": title, "date": date, "extra": extra,
            "creators": [{"creatorType": "author", "firstName": first_name, "lastName": last_name}]
        }));
        let public = serde_json::to_value(&result.record).unwrap();
        proptest::prop_assert!(public.get("raw").is_none());
        proptest::prop_assert_eq!(result.record.id.key.as_str(), "SYN00001");
        proptest::prop_assert!(Snapshot::new(vec![LibraryObject::Record(result.record)]).is_ok());
    }
}

#[test]
fn embedded_extra_url_hacks_cannot_expose_local_file_locations() {
    for extra in [
        "{:URL: file:///invented/private.pdf}",
        "Invented prefix {:URL: C:/invented/private.pdf}",
        "URL: \\\\invented\\private.pdf\nTitle: Invented title",
        "{:URL: ../private.pdf}{:issued: 2026}",
    ] {
        let data = mapped(json!({"extra": extra}));
        assert!(!serde_json::to_string(&data.record).unwrap().contains("private.pdf"));
        assert!(data.warnings.contains(&zotero::MappingWarning::PrivateUrl));
        assert!(
            serde_json::to_string(data.record.raw.as_ref().unwrap())
                .unwrap()
                .contains("private.pdf")
        );
    }
}

#[test]
fn canonical_extra_key_labels_do_not_hide_conflicts() {
    use ibis_core::contract::ModelError;
    for label in ["Citation Key", "citation-key", "citationKey", " Citation-Key "] {
        let record = mapped(json!({"citationKey": "Field2026", "extra": format!("{label}: Different2026")})).record;
        assert_eq!(record.citation_key(), Err(ModelError::ConflictingCitationKeys));
        assert_eq!(record.citation_keys.len(), 2);
    }
}

#[test]
fn printed_url_suppression_keeps_search_identifiers_and_bare_domains_survive() {
    use ibis_core::identifier::Identifier;
    let result =
        mapped(json!({"itemType": "journalArticle", "pages": "1–9", "url": "https://arxiv.org/abs/2401.00001v2"}));
    assert!(!result.record.csl.contains_key("URL"));
    assert_eq!(result.record.identifiers, vec![Identifier::Arxiv("2401.00001".into())]);
    for url in [
        "www.example.invalid/invented",
        "doi.org/10.0000/invented",
        "example.invalid/invented",
    ] {
        let result = mapped(json!({"url": url, "extra": format!("URL: {url}")}));
        assert_eq!(result.record.csl["URL"], url);
        assert!(!result.warnings.contains(&zotero::MappingWarning::PrivateUrl));
    }
}

#[test]
fn maximum_length_particle_and_date_inputs_have_bounded_work() {
    let started = std::time::Instant::now();
    let family = format!("{}Synthetic", "a ".repeat(100_000));
    let given = format!("Example {}a", "a ".repeat(100_000));
    let date = "jan-".repeat(60_000);
    let result =
        mapped(json!({"date": date, "creators": [{"creatorType": "author", "firstName": given, "lastName": family}]}));
    assert_eq!(result.record.csl["issued"], json!({"literal": date}));
    assert!(result.warnings.contains(&zotero::MappingWarning::DateParseLimit));
    assert_eq!(result.record.csl["author"][0]["family"], "Synthetic");
    assert_eq!(result.record.csl["author"][0]["given"], "Example");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn bare_document_filenames_stay_engine_only() {
    for url in [
        "private.pdf",
        "invented.bib",
        "notes.md",
        "sample.json",
        "invented.sqlite",
        "private.PDF",
    ] {
        let result = mapped(json!({"url": url, "extra": format!("{{:URL: {url}}}")}));
        let public = serde_json::to_string(&result.record).unwrap();
        assert!(!public.contains(url));
        assert!(result.warnings.contains(&zotero::MappingWarning::PrivateUrl));
        assert!(
            serde_json::to_string(result.record.raw.as_ref().unwrap())
                .unwrap()
                .contains(url)
        );
    }
}

#[test]
fn extra_url_scanning_tracks_citeproc_field_boundaries() {
    for extra in [
        "URL:\nURL: file:///invented/private.pdf",
        "URL: \nURL: \nURL: file:///invented/private.pdf",
        "{:issued: 2026}URL: file:///invented/private.pdf",
        "{:URL: ../a}URL: file:///invented/private.pdf",
        "{:issued: 2026}{:URL: https://example.invalid/work}URL: file:///invented/private.pdf",
    ] {
        let result = mapped(json!({"extra": extra}));
        assert!(!serde_json::to_string(&result.record).unwrap().contains("private.pdf"));
        assert!(result.warnings.contains(&zotero::MappingWarning::PrivateUrl));
    }
}

#[test]
fn file_transport_urls_and_inline_key_conflicts_are_not_hidden() {
    use ibis_core::contract::ModelError;
    for url in [
        "smb://invented/private.pdf",
        "afp://invented/private.pdf",
        "nfs:/invented/private.pdf",
        "jar:file:/invented/private.pdf",
    ] {
        let result = mapped(json!({"url": url, "extra": format!("{{:URL: {url}}}")}));
        assert!(!serde_json::to_string(&result.record).unwrap().contains("private.pdf"));
        assert!(result.warnings.contains(&zotero::MappingWarning::PrivateUrl));
    }
    for extra in [
        "{:citation-key: Extra2026}",
        "{:{:citation-key: Extra2026}",
        "{:citation-key: Invalid{:citation-key: Extra2026}",
    ] {
        let result = mapped(json!({"citationKey": "Field2026", "extra": extra}));
        assert_eq!(result.record.citation_key(), Err(ModelError::ConflictingCitationKeys));
    }
}

#[test]
fn private_extra_urls_remove_the_entire_line_including_braces_and_lone_cr() {
    for extra in [
        "URL: file:///C:/Users/Invented/AppData/{0000-INVENTED}/private.pdf",
        "URL: file:///invented/a}/private.pdf",
        "Title: Invented\nURL: file:///invented/a\rb/private.pdf",
        "{:URL: file:///invented/{x}/private.pdf}",
        "URL:\nfile:///invented/private.pdf",
        "{:URL:\nfile:///invented/private.pdf}",
    ] {
        let result = mapped(json!({"extra": extra}));
        let public = serde_json::to_string(&result.record).unwrap();
        assert!(!public.contains("private.pdf"), "private URL tail leaked");
        assert!(result.warnings.contains(&zotero::MappingWarning::PrivateUrl));
    }
    // citeproc's line ends only at `\n`; Unicode separators stay in the value.
    for (separator, prose) in [("\n", true), ("\u{2028}", false), ("\u{2029}", false)] {
        let result =
            mapped(json!({"extra": format!("URL: file:///invented/{{x}}/private.pdf{separator}Visible prose")}));
        let note = result.record.csl["note"].as_str().unwrap_or_default();
        assert!(!note.contains("private.pdf"));
        assert_eq!(note.contains("Visible prose"), prose);
    }
}
