//! Engine-owned library objects and immutable, fingerprinted snapshots.

use std::collections::BTreeMap;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Fingerprint(#[schemars(regex(pattern = r"^[0-9a-f]{64}$"))] String);

impl Fingerprint {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
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
pub enum SourceFormat {
    Bibtex,
    CslJson,
    Zotero,
}

/// Engine-only original payload. A Record deliberately never exports it to a
/// front end, since raw source records can contain local file locations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RawProvenance {
    pub format: SourceFormat,
    pub data: Value,
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
    /// Normalised CSL data; all variables may affect a chosen style.
    pub csl: BTreeMap<String, Value>,
    #[serde(default)]
    pub citation_keys: Vec<CitationKey>,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Search-only identifiers (for example arXiv eprints), never invented as
    /// printed CSL fields. They invalidate display/search, not citation picks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identifiers: Vec<crate::identifier::Identifier>,
    /// Kept in the engine, excluded from the public JSON projection and hashes.
    #[serde(skip)]
    #[schemars(skip)]
    pub raw: Option<RawProvenance>,
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
            Self::Record(record) => {
                #[derive(Serialize)]
                struct CiteRecord<'a> {
                    id: &'a ItemRef,
                    csl: &'a BTreeMap<String, Value>,
                    citation_keys: &'a [CitationKey],
                    aliases: &'a [String],
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

/// No mutable object access is exposed. Refresh constructs a new value while
/// existing picker sessions retain their original value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    objects: Vec<SnapshotObject>,
    fingerprints: Fingerprints,
}

impl Snapshot {
    pub fn new(mut objects: Vec<LibraryObject>) -> Result<Self, ModelError> {
        // Check every recursively shaped payload before any other validation
        // can return and drop it. Values assembled by sources do not have the
        // JSON reader's recursion limit, including engine-only raw provenance.
        let safe_depth = objects.iter().all(|object| match object {
            LibraryObject::Record(record) => record
                .csl
                .values()
                .chain(record.raw.iter().map(|raw| &raw.data))
                .all(|value| validate_value_depth(value).is_ok()),
            _ => true,
        });
        if !safe_depth {
            // serde_json::Value's ordinary destructor is recursive too. A
            // rejected payload must be dismantled without using that stack.
            drop_objects_iteratively(objects);
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
        let objects = objects
            .into_iter()
            .map(|object| {
                let fingerprints = object.fingerprints()?;
                Ok(SnapshotObject { object, fingerprints })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let cite: Vec<_> = objects
            .iter()
            .map(|item| (item.object.id(), &item.fingerprints.cite))
            .collect();
        let full: Vec<_> = objects
            .iter()
            .map(|item| (item.object.id(), &item.fingerprints.full))
            .collect();
        let fingerprints = Fingerprints {
            cite: digest_json(&cite)?,
            full: digest_json(&full)?,
        };
        Ok(Self { objects, fingerprints })
    }

    pub fn objects(&self) -> &[SnapshotObject] {
        &self.objects
    }
    pub fn fingerprints(&self) -> &Fingerprints {
        &self.fingerprints
    }
    pub fn get(&self, id: &ItemRef) -> Option<&SnapshotObject> {
        self.objects
            .binary_search_by(|item| item.object.id().cmp(id))
            .ok()
            .map(|index| &self.objects[index])
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

fn digest_json(value: &impl Serialize) -> Result<Fingerprint, ModelError> {
    fn canonical(value: Value) -> Value {
        match value {
            Value::Object(object) => Value::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
            value => value,
        }
    }
    let value = canonical(serde_json::to_value(value).map_err(|_| ModelError::InvalidObject)?);
    let bytes = serde_json::to_vec(&value).map_err(|_| ModelError::InvalidObject)?;
    Ok(Fingerprint::of_bytes(&bytes))
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
            if let Some(raw) = record.raw.take() {
                drop_value_iteratively(raw.data);
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
            id: id("work"),
            csl: BTreeMap::from([("type".into(), json!("book")), ("title".into(), json!("Invented work"))]),
            citation_keys: vec![CitationKey {
                value: "Example2026".into(),
                source: CitationKeySource::Bibtex,
            }],
            aliases: vec!["Earlier2026".into()],
            identifiers: vec![],
            raw: None,
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
        record.raw = Some(RawProvenance {
            format: SourceFormat::Bibtex,
            data: json!({"invented": "payload"}),
        });
        round_trip(record.raw.as_ref().unwrap());
        let public = serde_json::to_value(&record).unwrap();
        assert!(public.get("raw").is_none());
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
    fn provenance_sources_and_fixed_errors_round_trip() {
        for source in [SourceFormat::Bibtex, SourceFormat::CslJson, SourceFormat::Zotero] {
            round_trip(&source);
        }
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
