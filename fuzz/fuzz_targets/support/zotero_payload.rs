use std::collections::HashSet;

use ibis_core::library::SelectError;
use ibis_core::zotero::{Library, load_items};
use serde_json::{Value, json};

/// Exercise the public mapper, without a transport, network, or private API.
pub fn exercise(data: &[u8]) {
    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return;
    };
    // Non-arrays are malformed items too; do not silently discard those roots.
    let mut page = match value {
        Value::Array(items) => items,
        item => vec![item],
    };
    // A known-good neighbor must survive any malformed items before it. Pick
    // a key absent from the page so arbitrary duplicate keys cannot hide it.
    let mut sentinel_key = "FUZZGOOD".to_string();
    while page
        .iter()
        .any(|item| item.get("key").and_then(Value::as_str) == Some(&sentinel_key))
    {
        sentinel_key.push('X');
    }
    page.push(json!({
        "key": sentinel_key,
        "data": {"itemType": "book"},
        "csljson": {"type": "book", "title": "Synthetic surviving neighbor"}
    }));

    for library in [Library::User, Library::Group(42)] {
        let loaded = load_items(library, &page);
        assert_eq!(loaded, load_items(library, &page), "mapping must be deterministic");
        let mut seen = HashSet::new();
        let mut expected = Vec::new();
        // Independent per-item mapping checks that a malformed neighbor cannot
        // discard or change another record, and that the first duplicate wins.
        for item in &page {
            for reference in load_items(library, std::slice::from_ref(item)).references {
                if seen.insert(reference.id.native_key.clone()) {
                    expected.push(reference);
                }
            }
        }
        assert_eq!(loaded.references, expected, "page mapping changed an independent item");
        let sentinel = loaded
            .references
            .last()
            .unwrap_or_else(|| panic!("valid neighbor was discarded"));
        assert_eq!(sentinel.id.native_key, sentinel_key);
        assert_eq!(sentinel.title_plain().as_deref(), Some("Synthetic surviving neighbor"));
        for reference in &loaded.references {
            assert_eq!(reference.id.source, library.to_string());
            let csl = reference.to_csl_json();
            assert!(csl.is_object());
            let encoded = serde_json::to_vec(&csl).unwrap_or_else(|error| panic!("CSL serialization: {error}"));
            assert_eq!(serde_json::from_slice::<Value>(&encoded).ok(), Some(csl));
        }
        // Export must refuse exactly the records whose public citation IDs
        // collide; an unrelated error must not hide a mapper regression.
        let mut csl_ids = HashSet::new();
        let collision = loaded
            .references
            .iter()
            .any(|reference| !csl_ids.insert(reference.citekey.as_deref().unwrap_or(&reference.id.native_key)));
        match ibis_core::library::csl_items(&loaded.references) {
            Ok(csl) => {
                assert!(!collision, "export accepted duplicate citation IDs");
                assert_eq!(csl.len(), loaded.references.len());
                assert!(serde_json::to_vec(&csl).is_ok());
            }
            Err(SelectError::DuplicateKeys { .. }) => {
                assert!(collision, "export rejected unique citation IDs");
            }
            Err(error) => panic!("unexpected CSL export failure: {error}"),
        }
    }
}
