//! Prepared search data over an immutable contract snapshot; no legacy model conversion.
use std::sync::Arc;

use super::{MatchField, SearchDocument, Texts, by_identifier, key_eq, name_text, normalize, rank};
use crate::{
    contract::*,
    identifier,
    model::{Name, strip_markup},
};

pub struct RecordHit<'a> {
    pub item: &'a SnapshotObject,
    pub score: u32,
    pub matched: Vec<MatchField>,
    /// Some term matched only with one typo.
    pub typo: bool,
    /// Some terms did not match: one of the closest matches, given only when
    /// nothing matches every term.
    pub partial: bool,
}

struct Entry {
    position: usize,
    record: Arc<PreparedRecord>,
}
struct PreparedRecord {
    id: ItemRef,
    keys: Vec<String>,
    year: Option<i32>,
    identifiers: Vec<identifier::Identifier>,
}
impl SearchDocument for Entry {
    type Identity = ItemRef;
    type Source = LibraryId;
    fn identity(&self) -> &ItemRef {
        &self.record.id
    }
    fn source(&self) -> &LibraryId {
        &self.record.id.library
    }
    fn year(&self) -> Option<i32> {
        self.record.year
    }
    fn exact_key(&self, query: &str) -> bool {
        self.record.keys.iter().any(|key| key_eq(key, query))
    }
}

pub struct SnapshotIndex {
    snapshot: Arc<Snapshot>,
    entries: Vec<Entry>,
    texts: Texts,
}
impl SnapshotIndex {
    pub fn new(snapshot: Arc<Snapshot>) -> Self {
        Self::build(snapshot, None)
    }

    /// Reuse keys, identifiers and normalized text when complete content matches.
    pub fn updated(snapshot: Arc<Snapshot>, previous: &Self) -> Self {
        Self::build(snapshot, Some(previous))
    }

    fn build(snapshot: Arc<Snapshot>, previous: Option<&Self>) -> Self {
        let mut texts = Texts::default();
        let mut old_entries = previous
            .into_iter()
            .flat_map(|index| index.entries.iter().enumerate())
            .peekable();
        let entries = snapshot
            .objects()
            .iter()
            .enumerate()
            .filter_map(|(position, item)| {
                let LibraryObject::Record(record) = &item.object else {
                    return None;
                };
                while old_entries.peek().is_some_and(|(_, old)| old.record.id < record.id) {
                    old_entries.next();
                }
                if let Some((n, old)) = old_entries.peek()
                    && old.record.id == record.id
                    && let Some(previous) = previous
                    && previous.snapshot.objects()[old.position].fingerprints.full == item.fingerprints.full
                {
                    texts.copy_record(&previous.texts, *n);
                    return Some(Entry {
                        position,
                        record: old.record.clone(),
                    });
                }
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
                let names: Vec<Name> = record
                    .csl
                    .values()
                    .filter_map(serde_json::Value::as_array)
                    .flatten()
                    .filter_map(|value| {
                        let object = value.as_object()?;
                        let field = |key: &str| object.get(key).and_then(serde_json::Value::as_str).map(str::to_owned);
                        Some(Name {
                            family: field("family"),
                            given: field("given"),
                            literal: field("literal"),
                            dropping_particle: field("dropping-particle"),
                            non_dropping_particle: field("non-dropping-particle"),
                            suffix: field("suffix"),
                        })
                    })
                    .collect();
                let title = strip_markup(text("title"));
                let container = strip_markup(text("container-title"));
                texts.push(
                    [
                        normalize(&keys.join(" ")),
                        normalize(&title),
                        name_text(names.iter()),
                        year.map(|year| year.to_string()).unwrap_or_default(),
                        normalize(text("DOI")),
                        normalize(text("keyword")),
                        normalize(&container),
                        normalize(text("abstract")),
                    ],
                    &[
                        (1, &title),
                        (1, &strip_markup(text("title-short"))),
                        (5, text("keyword")),
                        (6, &container),
                        (6, &strip_markup(text("container-title-short"))),
                    ],
                );
                Some(Entry {
                    position,
                    record: Arc::new(PreparedRecord {
                        id: record.id.clone(),
                        year,
                        keys,
                        identifiers: identifier::of_record(record),
                    }),
                })
            })
            .collect();
        Self {
            snapshot,
            entries,
            texts: texts.shrink(),
        }
    }
    /// About how many bytes the index holds besides the snapshot itself.
    pub fn heap_bytes(&self) -> usize {
        self.texts.heap_bytes()
            + self.entries.capacity() * std::mem::size_of::<Entry>()
            + self
                .entries
                .iter()
                .map(|entry| {
                    std::mem::size_of::<PreparedRecord>()
                        + 2 * std::mem::size_of::<usize>()
                        + entry.record.id.key.capacity()
                        + entry.record.id.library.as_str().len()
                        + entry.record.keys.capacity() * std::mem::size_of::<String>()
                        + entry.record.keys.iter().map(String::capacity).sum::<usize>()
                        + entry.record.identifiers.capacity() * std::mem::size_of::<identifier::Identifier>()
                })
                .sum::<usize>()
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
            self.entries
                .iter()
                .map(|entry| (entry, entry.record.identifiers.as_slice())),
            query,
            limit,
            scope,
            stop,
        )? {
            Some(hits) => hits,
            None => rank(&self.entries, &self.texts, query, limit, scope, stop)?,
        };
        Some(
            hits.into_iter()
                .map(|hit| RecordHit {
                    item: &self.snapshot.objects()[hit.reference.position],
                    score: hit.score,
                    matched: hit.matched,
                    typo: hit.typo,
                    partial: hit.partial,
                })
                .collect(),
        )
    }
}
