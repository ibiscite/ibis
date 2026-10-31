//! Prepared search data over an immutable contract snapshot; no legacy model conversion.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{
    MatchField, SearchDocument, Texts, by_identifier, in_scope, key_eq, lower, name_text, normalize, pasted, rank,
};
use crate::{
    contract::*,
    identifier,
    model::{Name, strip_markup},
    scan::{Stopped, Work},
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

/// Exact matches in paste order, with unmatched explicit citation keys.
pub struct PastedList<'a> {
    pub hits: Vec<RecordHit<'a>>,
    pub missing: Vec<String>,
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
    /// `None` means cancellation; `Some(None)` means ordinary search should run.
    pub fn pasted_list(
        &self,
        query: &str,
        scope: Option<&[LibraryId]>,
        stop: &dyn Fn() -> bool,
    ) -> Option<Option<PastedList<'_>>> {
        if stop() {
            return None;
        }
        if query.len() > 64 * 1024 {
            return Some(None);
        }
        let mut work = Work::new(query.len(), stop);
        let parsed = (|| {
            if let Some(raw) = pasted::raw_entries(query, &mut work)?
                && let Some(list) = self.resolve_list(raw, scope, stop, true).ok_or(Stopped::Cancelled)?
            {
                return Ok(Some(list));
            }
            let Some(parsed) = pasted::entries(query, &mut work)? else {
                return Ok(None);
            };
            self.resolve_list(parsed, scope, stop, false).ok_or(Stopped::Cancelled)
        })();
        match parsed {
            Ok(list) => Some(list),
            Err(Stopped::Cancelled) => None,
            Err(Stopped::Limit) => Some(None),
        }
    }
    fn resolve_list(
        &self,
        mut parsed: pasted::Entries<'_>,
        scope: Option<&[LibraryId]>,
        stop: &dyn Fn() -> bool,
        raw: bool,
    ) -> Option<Option<PastedList<'_>>> {
        let mut keys: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut identifiers: BTreeMap<identifier::Identifier, Vec<usize>> = BTreeMap::new();
        for (n, entry) in parsed.entries.iter().enumerate() {
            match entry {
                pasted::Entry::Key { value, .. } => {
                    keys.entry(value.chars().flat_map(lower).collect()).or_default().push(n)
                }
                pasted::Entry::Identifier { value, .. } => identifiers.entry(value.clone()).or_default().push(n),
            }
        }
        let mut matches = vec![Vec::new(); parsed.entries.len()];
        let mut matched = vec![false; parsed.entries.len()];
        // One snapshot pass, independent of the number of pasted entries.
        for (n, record) in self.entries.iter().enumerate() {
            if n % super::STOP_CHECK_INTERVAL == 0 && stop() {
                return None;
            }
            if !in_scope(record, scope) {
                continue;
            }
            let mut first = None;
            let mut mark = |entry: usize, field| {
                matched[entry] = true;
                if first.is_none_or(|(previous, _)| entry < previous) {
                    first = Some((entry, field));
                }
            };
            for key in &record.record.keys {
                let key: String = key.chars().flat_map(lower).collect();
                for &entry in keys.get(&key).into_iter().flatten() {
                    mark(entry, MatchField::Citekey);
                }
            }
            for id in &record.record.identifiers {
                for &entry in identifiers.get(id).into_iter().flatten() {
                    mark(
                        entry,
                        if matches!(id, identifier::Identifier::Doi(_)) {
                            MatchField::Doi
                        } else {
                            MatchField::Identifier
                        },
                    );
                }
            }
            // Retain each record only once, even if every pasted entry names it.
            if let Some((entry, field)) = first {
                matches[entry].push((n, field));
            }
        }
        // Exact raw keys keep their interpretation even when followed by a
        // strict identifier. Only unmatched raw tokens may become identifiers;
        // a key that itself looks like a DOI still wins over the DOI record.
        if raw && matched.iter().any(|matched| !matched) {
            for (entry, matched) in parsed.entries.iter_mut().zip(&matched) {
                if stop() {
                    return None;
                }
                if !matched {
                    let pasted::Entry::Key {
                        value: std::borrow::Cow::Borrowed(text),
                        ..
                    } = entry
                    else {
                        return Some(None);
                    };
                    let Some(value) = identifier::parse_query(text) else {
                        return Some(None);
                    };
                    *entry = pasted::Entry::Identifier { text, value };
                }
            }
            return self.resolve_list(parsed, scope, stop, false);
        }
        let mut hits = Vec::new();
        let mut missing = Vec::new();
        let mut missing_keys = BTreeSet::new();
        let mut missing_identifiers = BTreeSet::new();
        for (position, (entry, found)) in parsed.entries.iter().zip(matches).enumerate() {
            if stop() {
                return None;
            }
            if !matched[position] {
                if !entry.explicit() {
                    return Some(None);
                }
                let first = match entry {
                    pasted::Entry::Key { value, .. } => {
                        missing_keys.insert(value.chars().flat_map(lower).collect::<String>())
                    }
                    pasted::Entry::Identifier { value, .. } => missing_identifiers.insert(value.clone()),
                };
                if first {
                    missing.push(entry.text().to_owned());
                }
            }
            for (n, field) in found {
                hits.push(RecordHit {
                    item: &self.snapshot.objects()[self.entries[n].position],
                    score: 3000,
                    matched: vec![field],
                    typo: false,
                    partial: false,
                });
            }
        }
        Some(Some(PastedList { hits, missing }))
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
