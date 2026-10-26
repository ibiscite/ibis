//! Streaming API/database mapping with shared duplicate diagnostics.

use super::*;

/// Turn Zotero items (each an object with `key`, `data` and `csljson`, as
/// the API returns them with `include=data,csljson`) into references.
///
/// An item key seen twice keeps the first item. A citation key shared by
/// several items keeps them all, since their identity differs, but is
/// reported because a citation would be ambiguous.
pub fn load_items(library: Library, items: &[Value]) -> LoadResult {
    load_items_iter(library, items)
}

pub(super) fn load_items_iter<T: std::borrow::Borrow<Value>>(
    library: Library,
    items: impl IntoIterator<Item = T>,
) -> LoadResult {
    let mut result = LoadResult::default();
    let mut seen = BTreeSet::new();
    for item in items {
        match load_item(library, item.borrow()) {
            Ok(Some((reference, problems))) => {
                let key = reference.id.native_key.clone();
                if !seen.insert(key.clone()) {
                    result.warnings.push(LoadWarning {
                        kind: WarningKind::DuplicateKey,
                        line: None,
                        message: format!("the Zotero item `{key}` appears twice; the first copy is used"),
                        key: Some(key),
                    });
                    continue;
                }
                result.warnings.extend(problems.into_iter().map(|message| LoadWarning {
                    kind: WarningKind::Field,
                    line: None,
                    key: Some(key.clone()),
                    message,
                }));
                result.references.push(reference);
            }
            Ok(None) => {}
            Err((key, message)) => result.warnings.push(LoadWarning {
                kind: WarningKind::Syntax,
                line: None,
                key,
                message,
            }),
        }
    }
    let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for reference in &result.references {
        if let Some(citekey) = &reference.citekey {
            owners.entry(citekey).or_default().push(&reference.id.native_key);
        }
    }
    let shared: Vec<LoadWarning> = owners
        .into_iter()
        .filter(|(_, items)| items.len() > 1)
        .map(|(citekey, items)| LoadWarning {
            kind: WarningKind::DuplicateKey,
            line: None,
            key: Some(citekey.to_string()),
            message: format!(
                "the citation key `{citekey}` is used by {} items ({}); citing it is ambiguous",
                items.len(),
                items.join(", ")
            ),
        })
        .collect();
    result.warnings.extend(shared);
    result
}
