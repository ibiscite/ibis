//! Several Zotero libraries read as one: the personal library and the groups
//! the user chose. Each library keeps its own version, so a change check
//! costs one small request per library, and a library that cannot be read is
//! reported by name while the others still load.

use serde_json::Value;

use super::Library;
use super::api::{self, Snapshot, Transport, TransportError, ZoteroError};
use crate::model::{LoadResult, LoadWarning, WarningKind};

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
            groups.push(group(entry).ok_or(ZoteroError::Incomplete)?);
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
    let name: String = entry
        .pointer("/data/name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_NAME)
        .collect();
    let name = name.trim();
    let name = if name.is_empty() {
        format!("Group {id}")
    } else {
        name.to_string()
    };
    Some(Group { id, name })
}

/// What one check of several libraries saw: each library's version, or
/// `None` for a library that could not be read and will be tried again.
pub type Snapshots = Vec<(Library, Option<Snapshot>)>;

/// The outcome of [`refresh`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refresh {
    /// No library changed and none that failed can be read now; keep the
    /// references already loaded.
    Unchanged,
    /// Every library read again, combined. A library that could not be read
    /// is a warning naming it.
    Loaded { result: LoadResult, snapshots: Snapshots },
}

/// Read `libraries` if any changed since `previous`, otherwise report that
/// nothing changed.
///
/// Errors that mean Zotero itself cannot be reached or used (not running,
/// access off, another API version) fail the whole read. Anything else
/// fails only that library, unless every library fails.
pub fn refresh(
    transport: &mut impl Transport,
    libraries: &[Library],
    previous: Option<&Snapshots>,
) -> Result<Refresh, ZoteroError> {
    // The same library chosen twice is read once.
    let mut unique: Vec<Library> = Vec::with_capacity(libraries.len());
    for &library in libraries {
        if !unique.contains(&library) {
            unique.push(library);
        }
    }
    let libraries = unique.as_slice();
    let before = |library: Library| {
        previous.and_then(|previous| {
            previous
                .iter()
                .find(|(seen, _)| *seen == library)
                .map(|(_, s)| s.as_ref())
        })
    };
    let mut outcomes = Vec::with_capacity(libraries.len());
    // A different choice or order of libraries is a change.
    let mut changed = previous.is_none_or(|previous| {
        !previous
            .iter()
            .map(|(library, _)| *library)
            .eq(libraries.iter().copied())
    });
    for &library in libraries {
        let outcome = match before(library) {
            // Unchanged so far: a cheap version check is enough.
            Some(Some(snapshot)) if !changed => api::refresh(transport, library, Some(snapshot)),
            _ => api::refresh(transport, library, None),
        };
        let outcome = match outcome {
            Err(error) if fatal(error) => return Err(error),
            outcome => outcome,
        };
        // A library that failed before and fails again is no change.
        let failed_again = matches!(before(library), Some(None)) && outcome.is_err();
        if !matches!(outcome, Ok(api::Refresh::Unchanged)) && !failed_again {
            changed = true;
        }
        outcomes.push((library, outcome));
    }
    if !changed {
        return Ok(Refresh::Unchanged);
    }
    let mut result = LoadResult::default();
    let mut snapshots = Snapshots::new();
    let mut first_error = None;
    for (library, outcome) in outcomes {
        let outcome = match outcome {
            // Checked before anything changed; its references are needed now.
            Ok(api::Refresh::Unchanged) => {
                api::scan(transport, library).map(|(result, snapshot)| api::Refresh::Loaded { result, snapshot })
            }
            outcome => outcome,
        };
        match outcome {
            Ok(api::Refresh::Loaded {
                result: loaded,
                snapshot,
            }) => {
                result.references.extend(loaded.references);
                result.warnings.extend(loaded.warnings);
                snapshots.push((library, Some(snapshot)));
            }
            Ok(api::Refresh::Unchanged) => return Err(ZoteroError::Incomplete),
            Err(error) if fatal(error) => return Err(error),
            Err(error) => {
                first_error.get_or_insert(error);
                result.warnings.push(LoadWarning {
                    kind: WarningKind::Syntax,
                    line: None,
                    key: None,
                    message: format!("{} was skipped: {error}", name(library)),
                });
                snapshots.push((library, None));
            }
        }
    }
    match first_error {
        Some(error) if snapshots.iter().all(|(_, snapshot)| snapshot.is_none()) => Err(error),
        _ => Ok(Refresh::Loaded { result, snapshots }),
    }
}

fn fatal(error: ZoteroError) -> bool {
    matches!(
        error,
        ZoteroError::Transport(TransportError::Unreachable) | ZoteroError::NotAllowed | ZoteroError::UnsupportedApi
    )
}

fn name(library: Library) -> String {
    match library {
        Library::User => "Your Zotero library".to_string(),
        Library::Group(id) => format!("Zotero group {id}"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;
    use crate::zotero::api::Response;

    fn ok(headers: &[(&str, String)], body: Value) -> Response {
        let mut all = vec![("Zotero-API-Version".to_string(), "3".to_string())];
        all.extend(headers.iter().map(|(n, v)| (n.to_string(), v.clone())));
        Response {
            status: 200,
            headers: all,
            body: serde_json::to_vec(&body).unwrap_or_default(),
        }
    }

    fn number(path: &str, name: &str) -> usize {
        path.split(['?', '&'])
            .find_map(|part| part.strip_prefix(name))
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    }

    /// A Zotero holding a few libraries, each a list of item keys at a
    /// version. A library missing from `libraries` answers 404.
    #[derive(Default)]
    struct Zotero {
        libraries: BTreeMap<String, (u64, Vec<String>)>,
        groups: Vec<Value>,
        fail_everything: Option<TransportError>,
        asked: Vec<String>,
    }

    impl Zotero {
        fn with(mut self, prefix: &str, version: u64, keys: &[&str]) -> Self {
            self.libraries.insert(
                prefix.to_string(),
                (version, keys.iter().map(|k| k.to_string()).collect()),
            );
            self
        }
    }

    impl Transport for Zotero {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            self.asked.push(path.to_string());
            if let Some(error) = self.fail_everything {
                return Err(error);
            }
            let (start, limit) = (number(path, "start="), number(path, "limit="));
            if path.starts_with("users/0/groups") {
                let end = (start + limit).min(self.groups.len());
                let page: Vec<Value> = self.groups[start.min(end)..end].to_vec();
                return Ok(ok(
                    &[("Total-Results", self.groups.len().to_string())],
                    Value::Array(page),
                ));
            }
            let prefix = path.split("/items").next().unwrap_or_default();
            let Some((version, keys)) = self.libraries.get(prefix) else {
                return Ok(Response {
                    status: 404,
                    ..Response::default()
                });
            };
            let version_header = ("Last-Modified-Version", version.to_string());
            if let Some(since) = path.split('&').find_map(|part| part.strip_prefix("since=")) {
                let changed = since.parse::<u64>().ok() != Some(*version);
                let body = if changed { json!({ "ITEM": version }) } else { json!({}) };
                return Ok(ok(&[version_header], body));
            }
            let end = (start + limit).min(keys.len());
            let items: Vec<Value> = keys[start.min(end)..end]
                .iter()
                .map(|key| {
                    let csl = json!([{ "type": "book", "title": key }]).to_string();
                    json!({ "key": key, "data": { "itemType": "book" }, "csljson": csl })
                })
                .collect();
            Ok(ok(
                &[version_header, ("Total-Results", keys.len().to_string())],
                Value::Array(items),
            ))
        }
    }

    fn loaded(refresh: Refresh) -> (LoadResult, Snapshots) {
        match refresh {
            Refresh::Loaded { result, snapshots } => (result, snapshots),
            Refresh::Unchanged => panic!("expected a read"),
        }
    }

    fn ids(result: &LoadResult) -> Vec<String> {
        result
            .references
            .iter()
            .map(|r| format!("{}/{}", r.id.source, r.id.native_key))
            .collect()
    }

    const BOTH: [Library; 2] = [Library::User, Library::Group(42)];

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
        assert_eq!(groups(&mut Short(short("0"))), Ok(Vec::new()));
    }

    #[test]
    fn several_libraries_load_together_and_keep_their_identity() {
        let mut zotero = Zotero::default()
            .with("users/0", 5, &["SAME", "MINE"])
            .with("groups/42", 9, &["SAME"]);
        let (result, snapshots) = loaded(refresh(&mut zotero, &BOTH, None).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(
            ids(&result),
            ["zotero:user/SAME", "zotero:user/MINE", "zotero:group:42/SAME"].map(String::from)
        );
        assert!(result.warnings.is_empty());
        let versions: Vec<_> = snapshots
            .iter()
            .map(|(l, s)| (*l, s.as_ref().map(|s| s.version)))
            .collect();
        assert_eq!(versions, [(Library::User, Some(5)), (Library::Group(42), Some(9))]);

        // Nothing changed: one version check per library and no reading.
        zotero.asked.clear();
        assert_eq!(refresh(&mut zotero, &BOTH, Some(&snapshots)), Ok(Refresh::Unchanged));
        assert_eq!(zotero.asked.len(), 2);
        assert!(zotero.asked.iter().all(|path| path.contains("since=")));

        // One group changed: everything is read again so nothing is lost.
        zotero
            .libraries
            .insert("groups/42".into(), (10, vec!["SAME".into(), "NEW".into()]));
        let (result, snapshots) =
            loaded(refresh(&mut zotero, &BOTH, Some(&snapshots)).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(result.references.len(), 4);
        assert_eq!(snapshots[1].1.as_ref().map(|s| s.version), Some(10));

        // A library chosen twice is read once.
        let twice = [Library::User, Library::User];
        let (result, _) = loaded(refresh(&mut zotero, &twice, None).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(result.references.len(), 2);

        // The same libraries in another order are read again in that order.
        let reordered = [Library::Group(42), Library::User];
        let (result, swapped) =
            loaded(refresh(&mut zotero, &reordered, Some(&snapshots)).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(result.references[0].id.source, "zotero:group:42");
        assert_eq!(swapped[0].0, Library::Group(42));

        // Choosing fewer libraries is a change, not "unchanged".
        let only_user = [Library::User];
        let (result, _) = loaded(refresh(&mut zotero, &only_user, Some(&snapshots)).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(result.references.len(), 2);
    }

    #[test]
    fn a_group_that_is_gone_is_named_and_the_rest_still_loads() {
        let mut zotero = Zotero::default().with("users/0", 5, &["MINE"]);
        let (result, snapshots) = loaded(refresh(&mut zotero, &BOTH, None).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(ids(&result), ["zotero:user/MINE"]);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.starts_with("Zotero group 42 was skipped: "));
        assert!(result.warnings[0].message.contains("not found"));
        assert_eq!(snapshots[1], (Library::Group(42), None));

        // Still gone: no change, and the personal library is not read again.
        zotero.asked.clear();
        assert_eq!(refresh(&mut zotero, &BOTH, Some(&snapshots)), Ok(Refresh::Unchanged));
        assert_eq!(zotero.asked.len(), 2);

        // Back again: everything is read and the warning goes away.
        zotero.libraries.insert("groups/42".into(), (3, vec!["THEIRS".into()]));
        let (result, _) = loaded(refresh(&mut zotero, &BOTH, Some(&snapshots)).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(result.references.len(), 2);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn a_library_that_times_out_is_skipped_but_others_still_load() {
        struct SlowGroup(Zotero);
        impl Transport for SlowGroup {
            fn get(&mut self, path: &str) -> Result<Response, TransportError> {
                if path.starts_with("groups/42") {
                    return Err(TransportError::TimedOut);
                }
                self.0.get(path)
            }
        }
        let mut zotero = SlowGroup(Zotero::default().with("users/0", 5, &["MINE"]));
        let (result, snapshots) = loaded(refresh(&mut zotero, &BOTH, None).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(result.references.len(), 1);
        assert!(result.warnings[0].message.starts_with("Zotero group 42 was skipped: "));
        assert_eq!(snapshots[1], (Library::Group(42), None));
    }

    #[test]
    fn zotero_unreachable_or_every_library_failing_is_an_error() {
        let mut down = Zotero {
            fail_everything: Some(TransportError::Unreachable),
            ..Zotero::default()
        };
        assert_eq!(
            refresh(&mut down, &BOTH, None),
            Err(ZoteroError::Transport(TransportError::Unreachable))
        );
        let mut empty = Zotero::default();
        assert_eq!(
            refresh(&mut empty, &[Library::Group(42)], None),
            Err(ZoteroError::GroupNotFound)
        );
    }
}
