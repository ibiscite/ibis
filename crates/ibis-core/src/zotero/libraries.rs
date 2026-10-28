//! Listing the Zotero group libraries the user can choose.

use serde_json::Value;

use super::Library;
use super::api::{self, Transport, ZoteroError};

/// Groups listed per request; the most Zotero returns at once.
const GROUP_PAGE: usize = 100;
/// Group list pages read before giving up: ten thousand groups.
const MAX_GROUP_PAGES: usize = 100;
/// Characters of a group name kept for display.
const MAX_NAME: usize = 200;

/// A Zotero group library the user belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: u64,
    pub name: String,
}

/// List the group libraries this Zotero knows, in Zotero's order.
pub fn groups(transport: &mut impl Transport) -> Result<Vec<Group>, ZoteroError> {
    let mut groups = Vec::new();
    for page in 0..MAX_GROUP_PAGES {
        let start = page * GROUP_PAGE;
        let response = api::request(
            transport,
            Library::User,
            &format!("users/0/groups?format=json&limit={GROUP_PAGE}&start={start}"),
        )?;
        let total: usize = response
            .header("Total-Results")
            .and_then(|t| t.parse().ok())
            .ok_or(ZoteroError::Incomplete)?;
        let Ok(Value::Array(entries)) = serde_json::from_slice::<Value>(&response.body) else {
            return Err(ZoteroError::Incomplete);
        };
        if entries.len() > GROUP_PAGE {
            return Err(ZoteroError::Incomplete);
        }
        let got = entries.len();
        for entry in &entries {
            let group = group(entry).ok_or(ZoteroError::Incomplete)?;
            // A group seen twice means the list moved between pages, so one
            // may also be missing: never return a list that may be partial.
            if groups.iter().any(|seen: &Group| seen.id == group.id) {
                return Err(ZoteroError::Incomplete);
            }
            groups.push(group);
        }
        if groups.len() >= total {
            return Ok(groups);
        }
        if got == 0 {
            // Fewer groups than Zotero counted: never return a partial list.
            return Err(ZoteroError::Incomplete);
        }
    }
    Err(ZoteroError::TooLarge)
}

fn group(entry: &Value) -> Option<Group> {
    let id = entry.get("id")?.as_u64()?;
    let name = clean_name(entry.pointer("/data/name").and_then(Value::as_str).unwrap_or_default());
    let name = if name.is_empty() { format!("Group {id}") } else { name };
    Some(Group { id, name })
}

/// A group name as Ibis shows it: without invisible or control characters,
/// trimmed and at most [`MAX_NAME`] characters long.
pub fn clean_name(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|&c| !invisible(c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_NAME)
        .collect();
    name.trim().to_owned()
}

/// Direction overrides and isolates and invisible spaces, which could make
/// a group name look like another or reorder the text around it. Zero-width
/// joiners and direction marks stay: they are part of correct spelling in
/// Persian, Arabic, Hebrew and many Indic scripts.
fn invisible(c: char) -> bool {
    matches!(c, '\u{200B}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::zotero::api::{Response, TransportError};
    use crate::zotero::test_support::number;

    fn ok(headers: &[(&str, String)], body: Value) -> Response {
        let mut all = vec![("Zotero-API-Version".to_string(), "3".to_string())];
        all.extend(headers.iter().map(|(n, v)| (n.to_string(), v.clone())));
        Response {
            status: 200,
            headers: all,
            body: serde_json::to_vec(&body).unwrap_or_default(),
        }
    }

    #[derive(Default)]
    struct Zotero {
        groups: Vec<Value>,
        asked: Vec<String>,
    }

    impl Transport for Zotero {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            assert!(path.starts_with("users/0/groups?"));
            self.asked.push(path.to_string());
            let (start, limit) = (number(path, "start="), number(path, "limit="));
            let end = (start + limit).min(self.groups.len());
            let page: Vec<Value> = self.groups[start.min(end)..end].to_vec();
            Ok(ok(
                &[("Total-Results", self.groups.len().to_string())],
                Value::Array(page),
            ))
        }
    }

    #[test]
    fn groups_are_listed_page_by_page_with_safe_names() {
        let mut zotero = Zotero {
            groups: (0..GROUP_PAGE as u64 + 2)
                .map(|id| json!({ "id": id + 1, "data": { "id": id + 1, "name": format!("Lab {id}") } }))
                .collect(),
            ..Zotero::default()
        };
        zotero.groups[1] = json!({ "id": 2, "data": { "name": "  Line\nbreak\u{7}  " } });
        zotero.groups[2] = json!({ "id": 3, "data": {} });
        zotero.groups[3] = json!({ "id": 4, "data": { "name": "x".repeat(500) } });
        zotero.groups[4] = json!({ "id": 5, "data": { "name": "\u{202E}baL\u{200B} \u{2066}A\u{2069} \u{645}\u{6CC}\u{200C}\u{62E}" } });
        let listed = groups(&mut zotero).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(listed.len(), GROUP_PAGE + 2);
        assert_eq!(
            listed[0],
            Group {
                id: 1,
                name: "Lab 0".into()
            }
        );
        assert_eq!(listed[1].name, "Line break");
        assert_eq!(listed[2].name, "Group 3");
        assert_eq!(listed[3].name.chars().count(), MAX_NAME);
        assert_eq!(listed[4].name, "baL A \u{645}\u{6CC}\u{200C}\u{62E}");
        assert_eq!(zotero.asked.len(), 2);
    }

    #[test]
    fn a_group_list_without_ids_or_shorter_than_counted_is_an_error() {
        let mut zotero = Zotero {
            groups: vec![json!({ "data": { "name": "No id" } })],
            ..Zotero::default()
        };
        assert_eq!(groups(&mut zotero), Err(ZoteroError::Incomplete));

        let short = |total: &str| {
            let mut answer = ok(&[("Total-Results", total.to_string())], json!([]));
            answer.status = 200;
            answer
        };
        struct Short(Response);
        impl Transport for Short {
            fn get(&mut self, _: &str) -> Result<Response, TransportError> {
                Ok(self.0.clone())
            }
        }
        assert_eq!(groups(&mut Short(short("3"))), Err(ZoteroError::Incomplete));

        let mut repeated = Zotero {
            groups: vec![json!({ "id": 1, "data": {} }), json!({ "id": 1, "data": {} })],
            ..Zotero::default()
        };
        assert_eq!(groups(&mut repeated), Err(ZoteroError::Incomplete));
        assert_eq!(groups(&mut Short(short("0"))), Ok(Vec::new()));
    }
}
