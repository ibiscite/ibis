//! Synthetic regressions for silent field omissions (#342).

use ibis_core::model::WarningKind;
use ibis_core::zotero::{Library, load_items};
use serde_json::{Value, json};

fn item(csl: Value, local_api: bool) -> Value {
    let csl = if local_api {
        json!(json!([csl]).to_string())
    } else {
        csl
    };
    json!({"key": "SYN00001", "data": {"itemType": "book"}, "csljson": csl})
}

#[test]
#[ignore = "#342: malformed dedicated CSL text fields disappear without field warnings"]
fn malformed_text_fields_report_what_was_skipped_and_keep_readable_metadata() {
    let mut missing_warnings = Vec::new();
    for local_api in [false, true] {
        for field in ["title", "container-title", "DOI", "URL", "abstract", "language"] {
            let mut csl = json!({
                "type": "book", "title": "Synthetic title", "publisher": "Synthetic publisher",
                "author": [{"given": "Visible", "family": "Synthetic"}]
            });
            csl[field] = json!({"unexpected": "Synthetic text"});
            let loaded = load_items(Library::User, &[item(csl, local_api)]);
            assert_eq!(loaded.references.len(), 1);
            let reference = &loaded.references[0];
            assert_eq!(reference.id.source, "zotero:user");
            assert_eq!(reference.id.native_key, "SYN00001");
            assert_eq!(reference.variables["publisher"], "Synthetic publisher");
            assert_eq!(reference.authors[0].given.as_deref(), Some("Visible"));
            assert!(reference.to_csl_json().get(field).is_none());
            if !loaded.warnings.iter().any(|warning| {
                warning.kind == WarningKind::Field
                    && warning.key.as_deref() == Some("SYN00001")
                    && warning.message.contains(field)
            }) {
                missing_warnings.push((local_api, field));
            }
        }
    }
    assert!(
        missing_warnings.is_empty(),
        "fields omitted without a field warning: {missing_warnings:?}"
    );
}

#[test]
#[ignore = "#342: an unreadable family component silently disappears from a partially valid name"]
fn malformed_family_names_are_reported_while_given_names_and_titles_survive() {
    for local_api in [false, true] {
        let csl = json!({
            "type": "book", "title": "Synthetic title",
            "author": [{"family": ["Synthetic"], "given": "Visible"}]
        });
        let loaded = load_items(Library::User, &[item(csl, local_api)]);
        assert_eq!(loaded.references.len(), 1);
        let reference = &loaded.references[0];
        assert_eq!(reference.id.native_key, "SYN00001");
        assert_eq!(reference.title_plain().as_deref(), Some("Synthetic title"));
        assert_eq!(reference.authors.len(), 1);
        assert_eq!(reference.authors[0].given.as_deref(), Some("Visible"));
        assert_eq!(reference.authors[0].family, None);
        assert!(loaded.warnings.iter().any(|warning| {
            warning.kind == WarningKind::Field
                && warning.key.as_deref() == Some("SYN00001")
                && (warning.message.contains("author") || warning.message.contains("family"))
        }));
    }
}
