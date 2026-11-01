//! Engine-owned library objects and immutable, fingerprinted snapshots.

use std::{collections::BTreeMap, sync::Arc};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// A source identity digest, never a source location. Sources hash the full
/// canonical file identity, or profile/data-directory/library identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
pub struct LibraryId(#[schemars(regex(pattern = r"^(file|zotero):[0-9a-f]{64}$"))] String);

impl LibraryId {
    pub fn new(value: &str) -> Result<Self, ModelError> {
        let valid = value
            .strip_prefix("file:")
            .or_else(|| value.strip_prefix("zotero:"))
            .is_some_and(is_digest);
        if valid {
            Ok(Self(value.into()))
        } else {
            Err(ModelError::InvalidLibraryId)
        }
    }

    pub fn file(identity: &[u8]) -> Self {
        Self(format!("file:{}", Fingerprint::of_bytes(identity).as_str()))
    }

    pub fn zotero(identity: &[u8]) -> Self {
        Self(format!("zotero:{}", Fingerprint::of_bytes(identity).as_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for LibraryId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(&String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Identity inside one library; citation keys are intentionally separate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// Non-item kinds use source-namespaced keys (for example `tag:...`) so
/// source tags and collections cannot collide with native item keys.
pub struct ItemRef {
    pub library: LibraryId,
    pub key: String,
}

/// Lowercase hexadecimal, the form every fingerprint has always used.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Fingerprint(#[schemars(regex(pattern = r"^[0-9a-f]{64}$"))] String);

impl Fingerprint {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(hex(&Sha256::digest(bytes)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if is_digest(&value) {
            Ok(Self(value))
        } else {
            Err(de::Error::custom(ModelError::InvalidFingerprint))
        }
    }
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fingerprints {
    pub cite: Fingerprint,
    pub full: Fingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CitationKeySource {
    Bibtex,
    CslJsonCitationKey,
    CslJsonId,
    Zotero,
    Extra,
    BetterBibtex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CitationKey {
    pub value: String,
    pub source: CitationKeySource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: ItemRef,
    /// Source-added metadata; affects recents/display, never citation meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_at: Option<String>,
    /// Normalised CSL data; all variables may affect a chosen style.
    pub csl: BTreeMap<String, Value>,
    #[serde(default)]
    pub citation_keys: Vec<CitationKey>,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Item-local conversion uncertainty, independent of CSL and source paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<super::CitationWarningCode>,
    /// Search-only identifiers (for example arXiv eprints), never invented as
    /// printed CSL fields. They invalidate display/search, not citation picks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identifiers: Vec<crate::identifier::Identifier>,
}

impl Record {
    /// Agreement is required; provenance never determines which key wins.
    pub fn citation_key(&self) -> Result<Option<&str>, ModelError> {
        let Some(first) = self.citation_keys.first() else {
            return Ok(None);
        };
        if self.citation_keys.iter().any(|key| key.value != first.value) {
            Err(ModelError::ConflictingCitationKeys)
        } else if first.value.is_empty() || first.value.chars().any(|c| c.is_whitespace() || c.is_control()) {
            Err(ModelError::InvalidCitationKey)
        } else {
            Ok(Some(&first.value))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    OnDisk,
    CloudPlaceholder,
    NotDownloaded,
    Online,
    BrokenLink,
    Unreadable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub id: ItemRef,
    pub parent: Option<ItemRef>,
    pub title: String,
    pub media_type: String,
    pub availability: Availability,
    /// Identifies the file contents without exposing a path or attachment URL.
    pub content_hash: Option<Fingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: ItemRef,
    pub parent: Option<ItemRef>,
    /// Untrusted plain text; front ends must never inject this as HTML.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub id: ItemRef,
    /// The attachment, not an invented bibliographic parent.
    pub parent: ItemRef,
    pub text: String,
    pub comment: String,
    pub page_label: Option<String>,
    /// Zero-based physical index, distinct from the printed page label.
    pub page_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    pub id: ItemRef,
    pub parent: Option<ItemRef>,
    pub name: String,
    pub items: Vec<ItemRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tag {
    pub id: ItemRef,
    pub name: String,
    #[serde(default)]
    pub items: Vec<ItemRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchCondition {
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SavedSearch {
    pub id: ItemRef,
    pub name: String,
    /// Source conditions are data only; this contract never executes them.
    pub conditions: Vec<SearchCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "data", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LibraryObject {
    Record(Record),
    Attachment(Attachment),
    Note(Note),
    Annotation(Annotation),
    Collection(Collection),
    Tag(Tag),
    SavedSearch(SavedSearch),
}

impl LibraryObject {
    pub fn id(&self) -> &ItemRef {
        match self {
            Self::Record(object) => &object.id,
            Self::Attachment(object) => &object.id,
            Self::Note(object) => &object.id,
            Self::Annotation(object) => &object.id,
            Self::Collection(object) => &object.id,
            Self::Tag(object) => &object.id,
            Self::SavedSearch(object) => &object.id,
        }
    }

    fn fingerprints(&self) -> Result<Fingerprints, ModelError> {
        let full = digest_json(self)?;
        let cite = match self {
            // With no search-only fields, both views serialize identically.
            Self::Record(record) if record.identifiers.is_empty() && record.added_at.is_none() => full.clone(),
            Self::Record(record) => {
                #[derive(Serialize)]
                struct CiteRecord<'a> {
                    id: &'a ItemRef,
                    csl: &'a BTreeMap<String, Value>,
                    citation_keys: &'a [CitationKey],
                    aliases: &'a [String],
                    #[serde(skip_serializing_if = "<[super::CitationWarningCode]>::is_empty")]
                    warnings: &'a [super::CitationWarningCode],
                }
                #[derive(Serialize)]
                struct CiteObject<'a> {
                    kind: &'static str,
                    data: CiteRecord<'a>,
                }
                digest_json(&CiteObject {
                    kind: "record",
                    data: CiteRecord {
                        id: &record.id,
                        csl: &record.csl,
                        citation_keys: &record.citation_keys,
                        aliases: &record.aliases,
                        warnings: &record.warnings,
                    },
                })?
            }
            // Availability and presentation metadata cannot change a citation.
            Self::Attachment(object) => digest_json(&(&object.id, &object.parent, &object.content_hash))?,
            _ => full.clone(),
        };
        Ok(Fingerprints { cite, full })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnapshotObject {
    pub object: LibraryObject,
    pub fingerprints: Fingerprints,
}

impl SnapshotObject {
    /// Validate and fingerprint one streamed object with the snapshot's rules.
    pub fn new(object: LibraryObject) -> Result<Self, ModelError> {
        Snapshot::prepare(vec![object])?
            .pop()
            .map(Arc::unwrap_or_clone)
            .ok_or(ModelError::InvalidObject)
    }
}

/// No mutable object access is exposed. Refresh constructs a new value while
/// existing picker sessions retain their original value.
// Objects are shared, so combining libraries never copies or re-fingerprints them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    objects: Arc<[Arc<SnapshotObject>]>,
    fingerprints: Fingerprints,
}

impl Snapshot {
    pub fn new(objects: Vec<LibraryObject>) -> Result<Self, ModelError> {
        Self::from_sorted(Self::prepare(objects)?)
    }

    /// Reuse validated objects by position without building a second payload.
    pub fn new_reusing(objects: Vec<LibraryObject>, previous: &Self, reused: &[usize]) -> Result<Self, ModelError> {
        if reused.is_empty() {
            return Self::new(objects);
        }
        let mut objects = Self::prepare(objects)?;
        objects.reserve(reused.len());
        for &position in reused {
            objects.push(
                previous
                    .objects
                    .get(position)
                    .ok_or(ModelError::InvalidItemRef)?
                    .clone(),
            );
        }
        objects.sort_unstable_by(|left, right| left.object.id().cmp(right.object.id()));
        if objects
            .windows(2)
            .any(|pair| pair[0].object.id() == pair[1].object.id())
        {
            return Err(ModelError::DuplicateItem);
        }
        Self::from_sorted(objects)
    }

    /// An identical record can be discarded immediately by a source adapter.
    pub fn matching_record(&self, record: &Record) -> Option<usize> {
        let position = self
            .objects
            .binary_search_by(|item| item.object.id().cmp(&record.id))
            .ok()?;
        // Value equality treats +0.0 and -0.0 as equal; JSON fingerprints do
        // not. Object iteration order matters too if preserve_order is enabled.
        fn same_encoding(left: &Value, right: &Value) -> bool {
            match (left, right) {
                (Value::Number(left), Value::Number(right)) => {
                    left.as_f64().map(f64::to_bits) == right.as_f64().map(f64::to_bits)
                }
                (Value::Array(left), Value::Array(right)) => left.iter().zip(right).all(|(a, b)| same_encoding(a, b)),
                (Value::Object(left), Value::Object(right)) => left
                    .iter()
                    .zip(right)
                    .all(|((a, av), (b, bv))| a == b && same_encoding(av, bv)),
                _ => true, // Lengths, keys, types and values already compared equal.
            }
        }
        matches!(&self.objects[position].object, LibraryObject::Record(old)
            if old == record && old.csl.values().zip(record.csl.values()).all(|(a, b)| same_encoding(a, b)))
        .then_some(position)
    }

    fn prepare(mut objects: Vec<LibraryObject>) -> Result<Vec<Arc<SnapshotObject>>, ModelError> {
        // Check every recursively shaped payload before any other validation
        // can return and drop it. Values assembled by sources do not have the
        // JSON reader's recursion limit.
        let safe_depth = objects.iter().all(|object| match object {
            LibraryObject::Record(record) => record.csl.values().all(|value| validate_value_depth(value).is_ok()),
            _ => true,
        });
        if !safe_depth {
            // serde_json::Value's ordinary destructor is recursive too. A
            // rejected payload must be dismantled without using that stack.
            drop_objects_iteratively(objects);
            return Err(ModelError::InvalidObject);
        }
        // Source uncertainty is one bounded fixed flag. Style expectations
        // belong to the formatter and cannot be supplied as source metadata.
        if objects.iter().any(|object| matches!(object, LibraryObject::Record(record)
            if record.warnings.len() > 1 || record.warnings.iter().any(|code| *code != super::CitationWarningCode::SourceField))) {
            return Err(ModelError::InvalidObject);
        }
        objects.sort_by(|left, right| left.id().cmp(right.id()));
        if objects.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(ModelError::DuplicateItem);
        }
        if objects
            .iter()
            .any(|object| object.id().key.trim().is_empty() || object.id().key.chars().any(char::is_control))
        {
            return Err(ModelError::InvalidItemRef);
        }
        objects
            .into_iter()
            .map(|object| {
                let fingerprints = object.fingerprints()?;
                Ok(Arc::new(SnapshotObject { object, fingerprints }))
            })
            .collect()
    }

    /// This snapshot's objects whose library passes `keep`, plus all of
    /// `other`'s. Both inputs were validated and fingerprinted already.
    pub fn merged(&self, other: &Snapshot, keep: impl Fn(&LibraryId) -> bool) -> Result<Self, ModelError> {
        self.merged_many(&[other], keep)
    }

    /// Combine a publication once, including its aggregate fingerprints.
    pub fn merged_many(&self, others: &[&Snapshot], keep: impl Fn(&LibraryId) -> bool) -> Result<Self, ModelError> {
        let kept = self.objects.iter().filter(|item| keep(&item.object.id().library));
        let count = kept.clone().count();
        if count == 0
            && let [other] = others
        {
            return Ok((*other).clone());
        }
        if others.is_empty() && count == self.objects.len() {
            return Ok(self.clone());
        }
        let mut objects = Vec::with_capacity(count + others.iter().map(|other| other.objects.len()).sum::<usize>());
        objects.extend(kept.cloned());
        for other in others {
            objects.extend(other.objects.iter().cloned());
        }
        objects.sort_unstable_by(|left, right| left.object.id().cmp(right.object.id()));
        if objects
            .windows(2)
            .any(|pair| pair[0].object.id() == pair[1].object.id())
        {
            return Err(ModelError::DuplicateItem);
        }
        Self::from_sorted(objects)
    }

    fn from_sorted(objects: Vec<Arc<SnapshotObject>>) -> Result<Self, ModelError> {
        let cite: Vec<_> = objects
            .iter()
            .map(|item| (item.object.id(), &item.fingerprints.cite))
            .collect();
        let cite = digest_json(&cite)?;
        let full = if objects
            .iter()
            .all(|item| item.fingerprints.cite == item.fingerprints.full)
        {
            cite.clone()
        } else {
            digest_json(
                &objects
                    .iter()
                    .map(|item| (item.object.id(), &item.fingerprints.full))
                    .collect::<Vec<_>>(),
            )?
        };
        let fingerprints = Fingerprints { cite, full };
        Ok(Self {
            objects: objects.into(),
            fingerprints,
        })
    }

    /// Retain the old allocation for objects whose complete content is unchanged.
    /// Both snapshots have already been validated; their fingerprints stay valid.
    pub fn reuse_unchanged(&mut self, previous: &Snapshot) {
        let mut old = previous.objects.iter().peekable();
        for position in 0..self.objects.len() {
            let item = &self.objects[position];
            while old.peek().is_some_and(|old| old.object.id() < item.object.id()) {
                old.next();
            }
            if let Some(old) = old.peek()
                && old.object.id() == item.object.id()
                && old.fingerprints.full == item.fingerprints.full
                && !Arc::ptr_eq(old, item)
            {
                let old = Arc::clone(old);
                Arc::make_mut(&mut self.objects)[position] = old;
            }
        }
    }

    pub fn objects(&self) -> &[Arc<SnapshotObject>] {
        &self.objects
    }
    pub fn fingerprints(&self) -> &Fingerprints {
        &self.fingerprints
    }
    pub fn get(&self, id: &ItemRef) -> Option<&SnapshotObject> {
        self.objects
            .binary_search_by(|item| item.object.id().cmp(id))
            .ok()
            .map(|index| &*self.objects[index])
    }
}

impl<'de> Deserialize<'de> for Snapshot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            objects: Vec<SnapshotObject>,
            fingerprints: Fingerprints,
        }
        let wire = Wire::deserialize(deserializer)?;
        // Keep only shallow validation metadata: cloning the payload itself
        // could overflow before new() gets to reject excessive nesting.
        let expected: Vec<_> = wire
            .objects
            .iter()
            .map(|item| (item.object.id().clone(), item.fingerprints.clone()))
            .collect();
        let snapshot =
            Self::new(wire.objects.into_iter().map(|item| item.object).collect()).map_err(de::Error::custom)?;
        let actual = snapshot
            .objects
            .iter()
            .map(|item| (item.object.id(), &item.fingerprints));
        if snapshot.fingerprints != wire.fingerprints
            || actual.ne(expected.iter().map(|(id, fingerprints)| (id, fingerprints)))
        {
            return Err(de::Error::custom(ModelError::InvalidFingerprint));
        }
        Ok(snapshot)
    }
}

/// SHA-256 of the value's JSON, streamed without an intermediate copy. The
/// text is canonical because struct fields serialize in declaration order and
/// `serde_json::Map` is a `BTreeMap` here (no `preserve_order` feature).
fn digest_json(value: &impl Serialize) -> Result<Fingerprint, ModelError> {
    struct Digest(Sha256);
    impl std::io::Write for Digest {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // serde emits many tiny writes. Batch them without changing a single byte
    // of the JSON or materializing a whole-library JSON value.
    let mut digest = std::io::BufWriter::new(Digest(Sha256::new()));
    serde_json::to_writer(&mut digest, value).map_err(|_| ModelError::InvalidObject)?;
    let digest = digest.into_inner().map_err(|_| ModelError::InvalidObject)?;
    Ok(Fingerprint(hex(&digest.0.finalize())))
}

fn validate_value_depth(value: &Value) -> Result<(), ModelError> {
    let mut pending = vec![(value, 0_u8)];
    while let Some((value, depth)) = pending.pop() {
        if depth > 64 {
            return Err(ModelError::InvalidObject);
        }
        match value {
            Value::Array(values) => pending.extend(values.iter().map(|value| (value, depth + 1))),
            Value::Object(object) => pending.extend(object.values().map(|value| (value, depth + 1))),
            _ => {}
        }
    }
    Ok(())
}

fn drop_objects_iteratively(mut objects: Vec<LibraryObject>) {
    for object in &mut objects {
        if let LibraryObject::Record(record) = object {
            for value in std::mem::take(&mut record.csl).into_values() {
                drop_value_iteratively(value);
            }
        }
    }
}

fn drop_value_iteratively(value: Value) {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(values) => pending.extend(values),
            Value::Object(object) => pending.extend(object.into_values()),
            _ => {}
        }
    }
}

/// Fixed errors deliberately omit source data and locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ModelError {
    #[error("invalid library identity")]
    InvalidLibraryId,
    #[error("invalid item identity")]
    InvalidItemRef,
    #[error("invalid fingerprint")]
    InvalidFingerprint,
    #[error("duplicate item identity")]
    DuplicateItem,
    #[error("conflicting citation keys")]
    ConflictingCitationKeys,
    #[error("invalid citation key")]
    InvalidCitationKey,
    #[error("invalid library object")]
    InvalidObject,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    #[test]
    fn merging_shares_objects_and_matches_a_fresh_snapshot() {
        let other = LibraryId::file(b"second invented library");
        let mut moved = record();
        moved.id.library = other.clone();
        let first = Snapshot::new(vec![LibraryObject::Record(record())]).unwrap();
        let second = Snapshot::new(vec![LibraryObject::Record(moved.clone())]).unwrap();
        let merged = first.merged(&second, |_| true).unwrap();
        let fresh = Snapshot::new(vec![LibraryObject::Record(record()), LibraryObject::Record(moved)]).unwrap();
        assert_eq!(merged, fresh);
        let empty = Snapshot::new(vec![]).unwrap();
        assert_eq!(empty.merged_many(&[&second, &first], |_| false).unwrap(), fresh);
        assert_eq!(merged.merged_many(&[], |_| true).unwrap(), merged);
        assert!(
            merged
                .objects()
                .iter()
                .any(|item| Arc::ptr_eq(item, &first.objects()[0]))
        );
        assert!(
            merged
                .objects()
                .iter()
                .any(|item| Arc::ptr_eq(item, &second.objects()[0]))
        );
        // Replacing a library drops its old objects; a clash is refused.
        let replaced = merged.merged(&second, |library| *library != other).unwrap();
        assert_eq!(replaced, merged);
        assert_eq!(merged.merged(&second, |_| true), Err(ModelError::DuplicateItem));
        let without = merged
            .merged(&Snapshot::new(vec![]).unwrap(), |library| *library != other)
            .unwrap();
        assert_eq!(without, first);
    }

    #[test]
    fn fingerprints_hash_canonical_json_with_sorted_keys() {
        // Map keys come out sorted whatever the insertion order; a dependency
        // enabling serde_json's `preserve_order` would break every fingerprint.
        let mut map = serde_json::Map::new();
        map.insert("zeta".into(), json!(1));
        map.insert("alpha".into(), json!({"y": 1, "b": [ {"d": 1, "c": 2} ]}));
        assert_eq!(
            serde_json::to_string(&map).unwrap(),
            r#"{"alpha":{"b":[{"c":2,"d":1}],"y":1},"zeta":1}"#
        );
        assert_eq!(
            digest_json(&map).unwrap(),
            Fingerprint::of_bytes(serde_json::to_string(&map).unwrap().as_bytes())
        );
        // Buffered writes, including values larger than the buffer, Unicode
        // and escaped bytes, must never change fingerprint values.
        let value = json!(["研究\n\\\"".repeat(10_000), map]);
        assert_eq!(
            digest_json(&value).unwrap(),
            Fingerprint::of_bytes(&serde_json::to_vec(&value).unwrap())
        );
    }

    #[test]
    fn reuse_keeps_unchanged_objects_and_preserves_fingerprints() {
        let unchanged = LibraryObject::Record(record());
        let mut changed = record();
        changed.id.key = "changed".into();
        let before = Snapshot::new(vec![unchanged.clone(), LibraryObject::Record(changed.clone())]).unwrap();
        let mut shared = before.clone();
        shared.reuse_unchanged(&before);
        assert!(Arc::ptr_eq(&shared.objects, &before.objects));
        changed.csl.insert("title".into(), json!("Changed synthetic title"));
        let fresh = Snapshot::new(vec![unchanged, LibraryObject::Record(changed.clone())]).unwrap();
        let mut reused = fresh.clone();
        reused.reuse_unchanged(&before);
        assert_eq!(reused, fresh);
        for (old, new) in before.objects().iter().zip(reused.objects()) {
            assert_eq!(Arc::ptr_eq(old, new), old.object.id() != &changed.id);
        }
        // The source and sole published library share the object slice itself.
        let empty = Snapshot::new(vec![]).unwrap();
        let installed = empty.merged_many(&[&reused], |_| false).unwrap();
        assert!(Arc::ptr_eq(&installed.objects, &reused.objects));
        assert_eq!(installed, fresh);
        let combined = empty.merged_many(&[&before, &fresh], |_| false);
        assert_eq!(combined, Err(ModelError::DuplicateItem));
    }

    #[test]
    fn reused_positions_are_validated_and_match_a_fresh_snapshot() {
        let keep = record();
        let previous = Snapshot::new(vec![LibraryObject::Record(keep.clone())]).unwrap();
        let position = previous.matching_record(&keep).unwrap();
        let mut added = keep.clone();
        added.id.key = "new".into();
        let next = Snapshot::new_reusing(vec![LibraryObject::Record(added.clone())], &previous, &[position]).unwrap();
        let fresh = Snapshot::new(vec![LibraryObject::Record(keep.clone()), LibraryObject::Record(added)]).unwrap();
        assert_eq!(next, fresh);
        assert!(
            next.objects()
                .iter()
                .any(|item| Arc::ptr_eq(item, &previous.objects()[0]))
        );
        assert_eq!(
            Snapshot::new_reusing(vec![], &previous, &[1]),
            Err(ModelError::InvalidItemRef)
        );
        assert_eq!(
            Snapshot::new_reusing(vec![], &previous, &[position, position]),
            Err(ModelError::DuplicateItem)
        );
        assert_eq!(
            Snapshot::new_reusing(vec![LibraryObject::Record(keep)], &previous, &[position]),
            Err(ModelError::DuplicateItem)
        );
    }

    #[test]
    fn equal_values_with_different_json_encodings_are_not_reused() {
        let mut positive = record();
        positive.csl.insert("custom".into(), json!({"nested":[0.0]}));
        let mut negative = positive.clone();
        negative.csl.insert("custom".into(), json!({"nested":[-0.0]}));
        assert_eq!(positive, negative);
        let previous = Snapshot::new(vec![LibraryObject::Record(positive)]).unwrap();
        assert!(previous.matching_record(&negative).is_none());
        let fresh = Snapshot::new(vec![LibraryObject::Record(negative)]).unwrap();
        assert_ne!(previous.fingerprints(), fresh.fingerprints());
    }

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T) {
        let wire = serde_json::to_vec(value).unwrap();
        assert_eq!(*value, serde_json::from_slice::<T>(&wire).unwrap());
    }

    fn id(key: &str) -> ItemRef {
        ItemRef {
            library: LibraryId::file(b"invented library"),
            key: key.into(),
        }
    }

    fn record() -> Record {
        Record {
            added_at: None,
            id: id("work"),
            csl: BTreeMap::from([("type".into(), json!("book")), ("title".into(), json!("Invented work"))]),
            citation_keys: vec![CitationKey {
                value: "Example2026".into(),
                source: CitationKeySource::Bibtex,
            }],
            aliases: vec!["Earlier2026".into()],
            warnings: vec![],
            identifiers: vec![],
        }
    }

    #[test]
    fn source_uncertainty_invalidates_citations_with_or_without_search_identifiers() {
        for identifiers in [
            vec![],
            vec![crate::identifier::parse_query("arXiv:2601.12345").unwrap()],
        ] {
            let mut clean = record();
            clean.identifiers = identifiers;
            let original = Snapshot::new(vec![LibraryObject::Record(clean.clone())]).unwrap();
            clean.warnings = vec![super::super::CitationWarningCode::SourceField];
            let uncertain = Snapshot::new(vec![LibraryObject::Record(clean.clone())]).unwrap();
            assert_ne!(original.fingerprints().cite, uncertain.fingerprints().cite);
            round_trip(&uncertain);
            for warnings in [
                vec![super::super::CitationWarningCode::SentenceCaseExpected],
                vec![super::super::CitationWarningCode::SourceField; 2],
            ] {
                clean.warnings = warnings;
                assert_eq!(
                    Snapshot::new(vec![LibraryObject::Record(clean.clone())]),
                    Err(ModelError::InvalidObject)
                );
            }
        }
    }

    #[test]
    fn opaque_ids_and_hashes_round_trip_without_paths() {
        let library = LibraryId::file(b"synthetic source identity");
        round_trip(&library);
        round_trip(&LibraryId::zotero(b"synthetic profile and library"));
        round_trip(&id("参考文献"));
        round_trip(&Fingerprint::of_bytes(b"synthetic"));
        for invalid in [
            "",
            "file:relative",
            "zotero:/invented/path",
            "file:../secret",
            "file:\\invented",
            "https://invalid.example",
        ] {
            assert_eq!(LibraryId::new(invalid), Err(ModelError::InvalidLibraryId));
            assert!(serde_json::from_value::<LibraryId>(json!(invalid)).is_err());
        }
        assert!(serde_json::from_value::<Fingerprint>(json!("invalid")).is_err());
        assert_eq!(serde_json::to_value(library).unwrap().as_str().unwrap().len(), 69);
    }

    #[test]
    fn citation_keys_keep_provenance_and_refuse_conflicts() {
        let mut record = record();
        round_trip(&record);
        round_trip(&record.citation_keys[0]);
        assert_eq!(record.citation_key(), Ok(Some("Example2026")));
        record.citation_keys.push(CitationKey {
            value: "Example2026".into(),
            source: CitationKeySource::Extra,
        });
        assert_eq!(record.citation_key(), Ok(Some("Example2026")));
        record.citation_keys.push(CitationKey {
            value: "Different2026".into(),
            source: CitationKeySource::BetterBibtex,
        });
        assert_eq!(record.citation_key(), Err(ModelError::ConflictingCitationKeys));
    }

    #[test]
    fn every_object_and_availability_round_trip() {
        let attachment = Attachment {
            id: id("file"),
            parent: None,
            title: "Invented attachment".into(),
            media_type: "application/pdf".into(),
            availability: Availability::Unknown,
            content_hash: None,
        };
        let note = Note {
            id: id("note"),
            parent: None,
            text: "Standalone invented note".into(),
        };
        let annotation = Annotation {
            id: id("annotation"),
            parent: id("file"),
            text: "Invented quotation".into(),
            comment: String::new(),
            page_label: Some("iv".into()),
            page_index: Some(3),
        };
        let collection = Collection {
            id: id("collection"),
            parent: None,
            name: "Invented collection".into(),
            items: vec![id("work")],
        };
        let tag = Tag {
            id: id("tag"),
            name: "Invented tag".into(),
            items: vec![id("work")],
        };
        let saved_search = SavedSearch {
            id: id("search"),
            name: "Invented search".into(),
            conditions: vec![SearchCondition {
                field: "title".into(),
                operator: "contains".into(),
                value: "Invented".into(),
            }],
        };
        round_trip(&attachment);
        round_trip(&note);
        round_trip(&annotation);
        round_trip(&collection);
        round_trip(&tag);
        round_trip(&saved_search);
        round_trip(&saved_search.conditions[0]);
        for state in [
            Availability::OnDisk,
            Availability::CloudPlaceholder,
            Availability::NotDownloaded,
            Availability::Online,
            Availability::BrokenLink,
            Availability::Unreadable,
            Availability::Unknown,
        ] {
            round_trip(&state);
        }
        let objects = vec![
            LibraryObject::Record(record()),
            LibraryObject::Attachment(attachment),
            LibraryObject::Note(note),
            LibraryObject::Annotation(annotation),
            LibraryObject::Collection(collection),
            LibraryObject::Tag(tag),
            LibraryObject::SavedSearch(saved_search),
        ];
        for object in &objects {
            round_trip(object);
        }
        let snapshot = Snapshot::new(objects).unwrap();
        round_trip(&snapshot);
        for object in snapshot.objects() {
            round_trip(object);
            round_trip(&object.fingerprints);
        }
        assert_eq!(snapshot.objects().len(), 7);
    }

    #[test]
    fn snapshot_fingerprints_distinguish_citation_from_display_changes() {
        let attachment = Attachment {
            id: id("file"),
            parent: Some(id("work")),
            title: "Invented".into(),
            media_type: "application/pdf".into(),
            availability: Availability::Unknown,
            content_hash: None,
        };
        let initial = Snapshot::new(vec![LibraryObject::Attachment(attachment.clone())]).unwrap();
        let changed = Snapshot::new(vec![LibraryObject::Attachment(Attachment {
            availability: Availability::OnDisk,
            ..attachment
        })])
        .unwrap();
        assert_eq!(initial.fingerprints().cite, changed.fingerprints().cite);
        assert_ne!(initial.fingerprints().full, changed.fingerprints().full);
        let mut changed = record();
        let before = Snapshot::new(vec![LibraryObject::Record(changed.clone())]).unwrap();
        changed.csl.insert("title".into(), json!("Changed invented title"));
        let after = Snapshot::new(vec![LibraryObject::Record(changed)]).unwrap();
        assert_ne!(before.fingerprints().cite, after.fingerprints().cite);
    }

    #[test]
    fn snapshots_reject_duplicates_and_forged_fingerprints() {
        let object = LibraryObject::Record(record());
        assert_eq!(
            Snapshot::new(vec![object.clone(), object.clone()]),
            Err(ModelError::DuplicateItem)
        );
        let snapshot = Snapshot::new(vec![object]).unwrap();
        let mut value = serde_json::to_value(snapshot).unwrap();
        value["objects"][0]["fingerprints"]["cite"] = json!("0".repeat(64));
        assert!(serde_json::from_value::<Snapshot>(value).is_err());
    }

    #[test]
    fn hostile_csl_nesting_is_refused_before_recursive_hashing() {
        let mut value = json!("invented");
        for _ in 0..128 {
            value = Value::Array(vec![value]);
        }
        let mut record = record();
        record.csl.insert("title".into(), value);
        assert_eq!(
            Snapshot::new(vec![LibraryObject::Record(record)]),
            Err(ModelError::InvalidObject)
        );
    }

    #[test]
    fn key_sources_and_fixed_errors_round_trip() {
        for source in [
            CitationKeySource::Bibtex,
            CitationKeySource::CslJsonCitationKey,
            CitationKeySource::CslJsonId,
            CitationKeySource::Zotero,
            CitationKeySource::Extra,
            CitationKeySource::BetterBibtex,
        ] {
            round_trip(&source);
        }
        for error in [
            ModelError::InvalidLibraryId,
            ModelError::InvalidItemRef,
            ModelError::InvalidFingerprint,
            ModelError::DuplicateItem,
            ModelError::ConflictingCitationKeys,
            ModelError::InvalidCitationKey,
            ModelError::InvalidObject,
        ] {
            round_trip(&error);
        }
    }

    #[test]
    fn empty_and_large_multilingual_snapshots_are_deterministic() {
        round_trip(&Snapshot::new(vec![]).unwrap());
        let objects: Vec<_> = (0..2000)
            .map(|i| {
                LibraryObject::Note(Note {
                    id: id(&format!("note-{i}")),
                    parent: None,
                    text: "Invented العربية 日本語 हिन्दी 😀".repeat(10),
                })
            })
            .collect();
        let first = Snapshot::new(objects.clone()).unwrap();
        let second = Snapshot::new(objects.into_iter().rev().collect()).unwrap();
        assert_eq!(first, second);
        round_trip(&first);
        let changed = first.objects()[0].clone();
        assert!(first.get(changed.object.id()).is_some());
    }
}
