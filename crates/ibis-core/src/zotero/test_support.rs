use serde_json::{Value, json};

pub(super) fn number(path: &str, name: &str) -> usize {
    path.split(['?', '&'])
        .find_map(|part| part.strip_prefix(name))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

pub(super) fn items(keys: &[impl AsRef<str>]) -> Vec<Value> {
    keys.iter()
        .map(|key| {
            let key = key.as_ref();
            // The local API sends CSL JSON as the text of its export, an array.
            let csl = json!([{ "type": "book", "title": key }]).to_string();
            json!({ "key": key, "data": { "itemType": "book" }, "csljson": csl })
        })
        .collect()
}
