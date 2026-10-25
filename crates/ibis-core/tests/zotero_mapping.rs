//! Synthetic source-preservation and warning-label regressions (#262).

use ibis_core::model::WarningKind;
use ibis_core::zotero::{Library, load_items};
use serde_json::{Value, json};

fn item(csl: Value, local: bool) -> Value {
    json!({
        "key": "SYN00001", "version": 7,
        "data": {"itemType": "book", "extra": "  Synthetic extra\n", "unknown": [1, null]},
        "links": {"attachment": {"attachmentType": "application/pdf", "href": "synthetic://unused"}},
        "csljson": if local { json!(json!([csl]).to_string()) } else { csl }
    })
}

#[test]
fn arbitrary_field_labels_never_enter_field_warning_messages() {
    let label = format!("SYNTHETIC_PRIVATE_LABEL\n`日本🌙{}", "x".repeat(4096));
    for local in [false, true] {
        for malformed in [
            json!({"unexpected": true}),
            json!([{}]),
            json!({"date-parts": [["unreadable"]]}),
            json!({"date-parts": [[2021, 21]]}),
            json!({"date-parts": [[2021, 2, 31]]}),
            json!({"date-parts": [[2021], ["unreadable"]]}),
        ] {
            let mut csl = json!({"type": "book", "title": "Synthetic title"});
            csl[&label] = malformed;
            let loaded = load_items(Library::User, &[item(csl, local)]);
            assert_eq!(loaded.references.len(), 1);
            assert!(!loaded.warnings.is_empty());
            for warning in loaded.warnings {
                assert_eq!(warning.kind, WarningKind::Field);
                assert_eq!(warning.key.as_deref(), Some("SYN00001"));
                assert!(!warning.message.contains("SYNTHETIC_PRIVATE_LABEL"));
                assert!(warning.message.len() < 128);
            }
        }
    }
}

#[test]
fn normalization_preserves_the_complete_raw_item_in_both_csl_encodings() {
    for local in [false, true] {
        let original = item(json!({
            "type": "book", "title": "  Été <I>日本🌙</I> & < 0.05  ",
            "container-title": "  <b >Synthetic</b>  ", "volume": 12,
            "abstract": "  Synthetic abstract  ", "custom": " original value "
        }), local);
        let loaded = load_items(Library::Group(42), std::slice::from_ref(&original));
        assert!(loaded.warnings.is_empty());
        let reference = &loaded.references[0];
        let raw: Value = serde_json::from_str(reference.raw.as_deref().unwrap()).unwrap();
        assert_eq!(raw, original);
        let csl = reference.to_csl_json();
        assert_eq!(csl["title"], "Été <i>日本🌙</i> &amp; &lt; 0.05");
        assert_eq!(csl["container-title"], "<b>Synthetic</b>");
        assert_eq!(csl["abstract"], "Synthetic abstract");
        assert_eq!(csl["volume"], "12");
        assert_eq!(csl["custom"], "original value");
    }
}

#[test]
fn title_scalars_keep_numeric_and_empty_behavior() {
    for local in [false, true] {
        for (value, expected) in [(json!(123), Some("123")), (json!("  "), None), (Value::Null, None)] {
            let loaded = load_items(Library::User, &[item(json!({
                "type": "book", "title": value, "container-title": value
            }), local)]);
            assert_eq!(loaded.references[0].title.as_deref(), expected);
            assert_eq!(loaded.references[0].container_title.as_deref(), expected);
            assert!(loaded.warnings.is_empty());
        }
    }
}
