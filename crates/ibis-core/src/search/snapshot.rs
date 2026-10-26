//! Prepared search data over an immutable contract snapshot; no legacy model conversion.
use std::sync::Arc;

use super::{MatchField, SearchDocument, by_identifier, key_eq, normalize, rank};
use crate::{
    contract::*,
    identifier,
    model::{Name, strip_markup},
};

pub struct RecordHit<'a> {
    pub item: &'a SnapshotObject,
    pub score: u32,
    pub matched: Vec<MatchField>,
}

struct Entry {
    position: usize,
    id: ItemRef,
    keys: Vec<String>,
    year: Option<i32>,
    fields: [String; 8],
    identifiers: Vec<identifier::Identifier>,
}
impl SearchDocument for Entry {
    type Identity = ItemRef;
    type Source = LibraryId;
    fn identity(&self) -> &ItemRef {
        &self.id
    }
    fn source(&self) -> &LibraryId {
        &self.id.library
    }
    fn year(&self) -> Option<i32> {
        self.year
    }
    fn exact_key(&self, query: &str) -> bool {
        self.keys.iter().any(|key| key_eq(key, query))
    }
}

pub struct SnapshotIndex {
    snapshot: Arc<Snapshot>,
    entries: Vec<Entry>,
}
impl SnapshotIndex {
    pub fn new(snapshot: Arc<Snapshot>) -> Self {
        let entries = snapshot
            .objects()
            .iter()
            .enumerate()
            .filter_map(|(position, item)| {
                let LibraryObject::Record(record) = &item.object else {
                    return None;
                };
                let keys: Vec<_> = record
                    .citation_keys
                    .iter()
                    .map(|key| key.value.clone())
                    .chain(record.aliases.iter().cloned())
                    .collect();
                let year = record
                    .csl
                    .get("issued")
                    .and_then(|date| date.get("date-parts"))
                    .and_then(|parts| parts.get(0))
                    .and_then(|parts| parts.get(0))
                    .and_then(serde_json::Value::as_i64)
                    .and_then(|year| i32::try_from(year).ok());
                let text = |field: &str| record.csl.get(field).and_then(serde_json::Value::as_str).unwrap_or("");
                let names = record
                    .csl
                    .values()
                    .filter_map(serde_json::Value::as_array)
                    .flatten()
                    .filter_map(|value| {
                        let object = value.as_object()?;
                        let field = |key: &str| object.get(key).and_then(serde_json::Value::as_str).map(str::to_owned);
                        Some(
                            Name {
                                family: field("family"),
                                given: field("given"),
                                literal: field("literal"),
                                dropping_particle: field("dropping-particle"),
                                non_dropping_particle: field("non-dropping-particle"),
                                suffix: field("suffix"),
                            }
                            .display(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                Some(Entry {
                    position,
                    id: record.id.clone(),
                    year,
                    fields: [
                        normalize(&keys.join(" ")),
                        normalize(&strip_markup(text("title"))),
                        normalize(&names),
                        year.map(|year| year.to_string()).unwrap_or_default(),
                        normalize(text("DOI")),
                        normalize(text("keyword")),
                        normalize(&strip_markup(text("container-title"))),
                        normalize(text("abstract")),
                    ],
                    keys,
                    identifiers: identifier::of_record(record),
                })
            })
            .collect();
        Self { snapshot, entries }
    }
    pub fn snapshot(&self) -> &Arc<Snapshot> {
        &self.snapshot
    }
    /// Scope and identifiers are applied before limits; cancelled searches return no partial list.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        scope: Option<&[LibraryId]>,
        stop: &dyn Fn() -> bool,
    ) -> Option<Vec<RecordHit<'_>>> {
        let hits = match by_identifier(
            self.entries.iter().map(|entry| (entry, entry.identifiers.as_slice())),
            query,
            limit,
            scope,
            stop,
        )? {
            Some(hits) => hits,
            None => rank(
                self.entries.iter().map(|entry| (entry, &entry.fields)),
                query,
                limit,
                scope,
                stop,
            )?,
        };
        Some(
            hits.into_iter()
                .map(|hit| RecordHit {
                    item: &self.snapshot.objects()[hit.reference.position],
                    score: hit.score,
                    matched: hit.matched,
                })
                .collect(),
        )
    }
}
