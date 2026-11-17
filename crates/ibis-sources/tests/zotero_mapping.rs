//! Invented data only. No Zotero process or library is used.
#![cfg(test)]
use ibis_sources::zotero;

use ibis_core::contract::LibraryId;
use serde_json::json;

#[test]
fn differential_against_pinned_zotero_covers_every_bibliographic_type() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../../fixtures/zotero/mapping/cases.json")).unwrap();
    let schema: serde_json::Value = serde_json::from_str(include_str!("../vendor/zotero/schema.json")).unwrap();
    let mut covered = std::collections::BTreeSet::new();
    let mut differences = Vec::new();
    for case in &cases {
        let item: zotero::ZoteroItem = serde_json::from_value(case["item"].clone()).unwrap();
        let mapped = zotero::map_item(LibraryId::digest(b"invented library"), &item).unwrap();
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
    zotero::map_item(LibraryId::digest(b"invented library"), &item(fields)).unwrap()
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
            zotero::map_item(LibraryId::digest(b"invented"), &item(json!({"itemType": item_type}))),
            Err(MappingError::UnsupportedType)
        );
    }
    for key in ["", "bad key", "bad\nkey"] {
        assert_eq!(
            zotero::map_item(LibraryId::digest(b"invented"), &item(json!({"key": key}))),
            Err(MappingError::InvalidIdentity)
        );
    }
    for fields in [
        json!({"title": "x".repeat(256 * 1024 + 1)}),
        json!({"creators": vec![json!({"creatorType":"author"}); 4097]}),
    ] {
        assert_eq!(
            zotero::map_item(LibraryId::digest(b"invented"), &item(fields)),
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
    assert_eq!(data.record.csl["title"], "日本語 &lt;script&gt;invented&lt;/script&gt;");
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
    }
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

struct SyntheticApi(Vec<serde_json::Value>);
impl ibis_core::zotero::api::Transport for SyntheticApi {
    fn get(&mut self, path: &str) -> Result<ibis_core::zotero::api::Response, ibis_core::zotero::api::TransportError> {
        let body = if path.ends_with("format=versions") {
            self.0
                .iter()
                .map(|row| (row["key"].as_str().unwrap().to_owned(), row["version"].clone()))
                .collect::<serde_json::Map<_, _>>()
                .into()
        } else {
            json!(self.0)
        };
        Ok(ibis_core::zotero::api::Response {
            status: 200,
            body: serde_json::to_vec(&body).unwrap(),
            headers: [
                ("Zotero-API-Version", "3"),
                ("X-Zotero-Version", "10.0.6"),
                ("Zotero-Server-ID", "invented-rules"),
                ("Last-Modified-Version", "1"),
            ]
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .chain([(String::from("Total-Results"), self.0.len().to_string())])
            .collect(),
        })
    }
}
fn api_item(key: &str, fields: serde_json::Value) -> serde_json::Value {
    let mut data = json!({"key": key, "version": 1, "itemType": "book", "dateModified": "2026-01-01T00:00:00Z"});
    data.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    json!({"key": key, "version": 1, "data": data})
}

#[test]
fn a_malformed_row_cannot_discard_a_valid_neighbor() {
    use ibis_sources::{WarningCode, zotero_source::ZoteroSource};
    for data in [
        json!(null),
        json!({}),
        json!({"key": "SYN00002", "version": 1, "itemType": "book"}),
    ] {
        for neighbors in [true, false] {
            let mut rows = vec![json!({"key": "SYN00002", "version": 1, "data": data})];
            if neighbors {
                rows.insert(0, api_item("SYN00001", json!({"title": "Invented first"})));
                rows.push(api_item("SYN00003", json!({"title": "Invented last"})));
            }
            let mut api = SyntheticApi(rows);
            let mut source = ZoteroSource::default();
            for _ in 0..2 {
                // Also exercise reuse of a malformed cached entry.
                let loaded = source.refresh(&mut api, false, &|| false).unwrap().remove(0);
                assert_eq!(loaded.snapshot.objects().len(), if neighbors { 2 } else { 0 });
                assert_eq!(loaded.warnings.len(), 1);
                assert_eq!(loaded.warnings[0].code, WarningCode::Field);
            }
        }
    }
}

#[test]
fn normalization_trims_and_escapes_rich_text() {
    for (input, expected) in [
        ("  Été <I>日本🌙</I> & < 0.05  ", "Été <i>日本🌙</i> &amp; &lt; 0.05"),
        ("\u{2003}<B><i >e\u{301}🌙</ i></B>\u{a0}", "<b><i>e\u{301}🌙</i></b>"),
        (
            "<Span Class=\"nocase\">x</SPAN><SUP>2</SUP>",
            "<span class=\"nocase\">x</span><sup>2</sup>",
        ),
        (
            "<span style=\"font-variant:small-caps;\">x</span>",
            "<span style=\"font-variant:small-caps;\">x</span>",
        ),
        (
            "<I><unknown>🌙</unknown></I><i x><ı>",
            "<i>&lt;unknown&gt;🌙&lt;/unknown&gt;</i>&lt;i x&gt;&lt;ı&gt;",
        ),
        (
            "&amp; &lt;x&gt; &#38; &#x1F319; &bare;",
            "&amp; &lt;x&gt; &#38; &#x1F319; &amp;bare;",
        ),
    ] {
        let result = mapped(json!({"title": input, "publicationTitle": input}));
        for field in ["title", "container-title"] {
            assert_eq!(result.record.csl[field], expected);
        }
    }
    for (input, expected) in [
        ("<".repeat(100_000), "&lt;".repeat(100_000)),
        ("&".repeat(100_000), "&amp;".repeat(100_000)),
        ("<I>".repeat(80_000), "<i>".repeat(80_000)),
        (
            format!("<i{}x", " ".repeat(100_000)),
            format!("&lt;i{}x", " ".repeat(100_000)),
        ),
    ] {
        assert_eq!(mapped(json!({"title": input})).record.csl["title"], expected);
    }
}

#[test]
fn blank_titles_publish_nothing() {
    for title in ["", "  \t\n", "\u{2003}\u{a0}\u{3000}"] {
        let record = mapped(json!({"title": title, "publicationTitle": title})).record;
        assert!(!record.csl.contains_key("title"));
        assert!(!record.csl.contains_key("container-title"));
    }
}

#[test]
fn numeric_titles_keep_their_digits() {
    let loaded = ibis_sources::zotero_source::ZoteroSource::default()
        .refresh(
            &mut SyntheticApi(vec![api_item("SYN00001", json!({"title": 1984}))]),
            false,
            &|| false,
        )
        .unwrap()
        .remove(0);
    assert!(loaded.warnings.is_empty());
    assert!(matches!(&loaded.snapshot.objects()[0].object,
        ibis_core::contract::LibraryObject::Record(record) if record.csl["title"] == "1984"));
}

#[test]
fn malformed_values_of_unknown_fields_are_reported() {
    for value in [
        json!(null),
        json!(false),
        json!(12),
        json!([1, null]),
        json!({"nested": [true]}),
    ] {
        let fields = json!({"title": "Invented title", "SYNTHETIC_PRIVATE_LABEL": value});
        let loaded = ibis_sources::zotero_source::ZoteroSource::default()
            .refresh(&mut SyntheticApi(vec![api_item("SYN00001", fields)]), false, &|| false)
            .unwrap()
            .remove(0);
        assert_eq!(loaded.snapshot.objects().len(), 1);
        assert_eq!(loaded.warnings[0].code, ibis_sources::WarningCode::Field);
        assert!(!format!("{:?}", loaded.warnings).contains("PRIVATE_LABEL"));
    }
}
