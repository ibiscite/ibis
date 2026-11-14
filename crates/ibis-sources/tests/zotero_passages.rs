//! Synthetic client scenarios, not Zotero goldens. The checked-in Zotero
//! differential corpus excludes annotations; no independently captured annotation
//! cases are available here. Live/closed acquisition and consent are not exercised.
#![allow(clippy::unwrap_used)]

use ibis_core::contract::{Annotation, ItemRef, LibraryId, LibraryObject};
use ibis_sources::zotero::{self, MappingError, ZoteroItem};
use serde_json::json;

fn library() -> LibraryId {
    LibraryId::new("synthetic-library").unwrap()
}

fn item(fields: serde_json::Value) -> ZoteroItem {
    let mut value = json!({
        "key": "SYN00001", "itemType": "annotation",
        "parentItem": "SYN00002", "annotationType": "highlight"
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

fn mapped(fields: serde_json::Value) -> Annotation {
    zotero::map_annotation(library(), &item(fields)).unwrap()
}

#[test]
fn stored_highlight_and_comment_remain_separate_and_exact() {
    let text = "  Invented <em>highlight</em> &amp; e\u{301}\r\n第二行  ";
    let comment = "\nMy invented <script>comment</script> & opinion.  ";
    for kind in ["highlight", "underline"] {
        let result = mapped(json!({
            "annotationType": kind, "annotationText": text, "annotationComment": comment
        }));
        assert_eq!(result.text, text);
        assert_eq!(result.comment, comment);
    }
    let comment_only = mapped(json!({"annotationComment": comment}));
    assert!(comment_only.text.is_empty());
    assert_eq!(comment_only.comment, comment);
    let empty = mapped(json!({}));
    assert!(empty.text.is_empty());
    assert!(empty.comment.is_empty());
}

#[test]
fn image_ink_and_comment_annotations_never_supply_quotation_text() {
    for kind in ["image", "ink", "note", "text"] {
        let result = mapped(json!({
            "annotationType": kind,
            "annotationText": "Invented stale text on a non-quotation annotation",
            "annotationComment": "  Invented comment, never a quotation.\n"
        }));
        assert!(result.text.is_empty());
        assert_eq!(result.comment, "  Invented comment, never a quotation.\n");
    }
}

#[test]
fn parent_is_the_supplied_attachment_in_the_supplied_library() {
    let source = item(json!({
        "parentItem": "ATT00001", "parentReference": "REF00001",
        "citationKey": "Invented2026", "libraryID": "untrusted-other-library"
    }));
    for library in [library(), LibraryId::new("another-synthetic-library").unwrap()] {
        let result = zotero::map_annotation(library.clone(), &source).unwrap();
        assert_eq!(
            result.id,
            ItemRef {
                library: library.clone(),
                key: "SYN00001".into()
            }
        );
        assert_eq!(
            result.parent,
            ItemRef {
                library,
                key: "ATT00001".into()
            }
        );
    }
}

#[test]
fn printed_labels_and_zero_based_physical_indices_are_independent() {
    for (label, index) in [("iii", 0), ("42", 7), ("", 3), ("  e\u{301}\u{202e} / 42  ", u32::MAX)] {
        let result = mapped(json!({
            "annotationPageLabel": label,
            "annotationPosition": json!({"pageIndex": index, "rects": [[1, 2, 3, 4]]}).to_string()
        }));
        assert_eq!(result.page_label.as_deref(), Some(label));
        assert_eq!(result.page_index, Some(index));
    }
    let no_label = mapped(json!({"annotationPosition": "{\"pageIndex\":0}"}));
    assert_eq!(no_label.page_label, None);
    assert_eq!(no_label.page_index, Some(0));
}

#[test]
fn missing_indices_stay_missing_even_for_numeric_or_roman_labels() {
    for label in ["42", "iv", "XII", "Ⅳ"] {
        for position in [None, Some("{}"), Some("{\"pageIndex\":null}"), Some("{\"rects\":[]}")] {
            let mut source = item(json!({"annotationPageLabel": label, "annotationSortIndex": "00003|000001"}));
            if let Some(position) = position {
                source.fields.insert("annotationPosition".into(), position.into());
            }
            let result = zotero::map_annotation(library(), &source).unwrap();
            assert_eq!(result.page_label.as_deref(), Some(label));
            assert_eq!(result.page_index, None);
        }
    }
}

#[test]
fn malformed_positions_return_one_fixed_diagnostic_without_payloads() {
    for position in [
        "",
        "invented-private-position",
        "null",
        "[]",
        "[7]",
        "42",
        "{\"pageIndex\":-1}",
        "{\"pageIndex\":1.5}",
        "{\"pageIndex\":1.0}",
        "{\"pageIndex\":\"iv\"}",
        "{\"pageIndex\":4294967296}",
        "{\"pageIndex\":true}",
        "{\"pageIndex\":{}}",
        "{\"pageIndex\":[]}",
        "{\"pageIndex\":1,\"pageIndex\":2}",
        "{\"pageIndex\":0,\"invented-private-position\":",
        "{} trailing",
    ] {
        let error = zotero::map_annotation(library(), &item(json!({"annotationPosition": position}))).unwrap_err();
        assert_eq!(error, MappingError::InvalidPosition);
        assert_eq!(error.to_string(), "invalid Zotero annotation position");
        assert_eq!(format!("{error:?}"), "InvalidPosition");
    }
}

#[test]
fn malformed_annotation_or_parent_identity_cannot_be_repaired_from_other_fields() {
    for key in [
        "".into(),
        "invented private identity".into(),
        "bad\nkey".into(),
        "x".repeat(129),
    ] {
        for field in ["key", "parentItem"] {
            let mut source = item(json!({"citationKey": "InventedFallback2026"}));
            if field == "key" {
                source.key = key.clone();
            } else {
                source.fields.insert(field.into(), key.clone());
            }
            let error = zotero::map_annotation(library(), &source).unwrap_err();
            assert_eq!(error, MappingError::InvalidIdentity);
            assert_eq!(error.to_string(), "invalid Zotero item identity");
            assert_eq!(format!("{error:?}"), "InvalidIdentity");
        }
    }
    let mut missing_parent = item(json!({}));
    missing_parent.fields.remove("parentItem");
    for source in [missing_parent, item(json!({"parentItem": "SYN00001"}))] {
        assert_eq!(
            zotero::map_annotation(library(), &source),
            Err(MappingError::InvalidIdentity)
        );
    }
}

#[test]
fn oversized_input_is_bounded_before_position_parsing_or_output_copying() {
    for field in [
        "annotationText",
        "annotationComment",
        "annotationPageLabel",
        "annotationPosition",
        "ignored",
    ] {
        let mut source = item(json!({}));
        source.fields.insert(field.into(), "x".repeat(256 * 1024 + 1));
        assert_eq!(
            zotero::map_annotation(library(), &source),
            Err(MappingError::InputLimit)
        );
    }
    let mut many_fields = item(json!({}));
    many_fields
        .fields
        .extend((0..257).map(|n| (format!("invented-{n}"), String::new())));
    let mut large_total = item(json!({}));
    large_total
        .fields
        .extend((0..9).map(|n| (format!("invented-{n}"), "x".repeat(256 * 1024))));
    let mut large_name = item(json!({}));
    large_name.fields.insert("x".repeat(256 * 1024 + 1), String::new());
    let mut many_creators = item(json!({}));
    many_creators.creators.resize(4097, Default::default());
    let mut many_keys = item(json!({}));
    many_keys.better_bibtex_keys.resize(4097, String::new());
    for source in [many_fields, large_total, large_name, many_creators, many_keys] {
        let error = zotero::map_annotation(library(), &source).unwrap_err();
        assert_eq!(error, MappingError::InputLimit);
        assert_eq!(error.to_string(), "Zotero mapping input limit exceeded");
        assert_eq!(format!("{error:?}"), "InputLimit");
    }
}

#[test]
fn both_future_adapters_can_supply_identical_neutral_input_and_get_identical_objects() {
    // Two constructions of neutral input, not a claim to exercise either reader.
    let from_strings = ZoteroItem {
        key: "SYN00001".into(),
        item_type: "annotation".into(),
        fields: [
            ("parentItem", "ATT00001"),
            ("annotationType", "highlight"),
            ("annotationText", "Invented highlight"),
            ("annotationComment", "Invented comment"),
            ("annotationPageLabel", "vii"),
            ("annotationPosition", "{\"pageIndex\":10}"),
        ]
        .map(|(key, value)| (key.into(), value.into()))
        .into(),
        ..Default::default()
    };
    let from_json = item(json!({
        "parentItem": "ATT00001", "annotationType": "highlight", "annotationText": "Invented highlight",
        "annotationComment": "Invented comment", "annotationPageLabel": "vii",
        "annotationPosition": "{\"pageIndex\":10}"
    }));
    assert_eq!(from_strings, from_json);
    let objects = [from_strings, from_json]
        .map(|source| LibraryObject::Annotation(zotero::map_annotation(library(), &source).unwrap()));
    assert_eq!(objects[0], objects[1]);
}

#[test]
fn annotation_mapping_does_not_open_the_bibliographic_publication_path() {
    for kind in ["attachment", "note", "annotation"] {
        assert_eq!(
            zotero::map_item(library(), &item(json!({"itemType": kind}))),
            Err(MappingError::UnsupportedType)
        );
    }
    for fields in [
        json!({"itemType": "book"}),
        json!({"annotationType": "unknown"}),
        json!({"annotationType": ""}),
    ] {
        assert_eq!(
            zotero::map_annotation(library(), &item(fields)),
            Err(MappingError::UnsupportedType)
        );
    }
    let mut missing_kind = item(json!({}));
    missing_kind.fields.remove("annotationType");
    assert_eq!(
        zotero::map_annotation(library(), &missing_kind),
        Err(MappingError::UnsupportedType)
    );
}
