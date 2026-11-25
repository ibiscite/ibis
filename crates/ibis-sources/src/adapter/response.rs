//! Parse one adapter answer before allocating its record payloads.

use ibis_core::contract::adapter::{AdapterRefusal, AdapterSkip};
use serde::de::{DeserializeSeed, Error, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

// The wire envelope, with payloads borrowed instead of the contract's owned
// Values. Only the shared CSL reader materializes the fields it understands.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Response<'a> {
    pub ibis: u32,
    #[serde(default, borrow)]
    pub read: Option<Read<'a>>,
    #[serde(default)]
    pub refused: Option<AdapterRefusal>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Read<'a> {
    #[serde(deserialize_with = "token")]
    pub scope: String,
    #[serde(deserialize_with = "token")]
    pub revision: String,
    #[serde(default)]
    pub keys: bool,
    #[serde(borrow, deserialize_with = "entries")]
    pub records: Vec<&'a RawValue>,
    #[serde(default, deserialize_with = "skipped")]
    pub skipped: Vec<AdapterSkip>,
}

pub(super) fn decode(bytes: &[u8]) -> serde_json::Result<Response<'_>> {
    serde_json::from_slice(bytes)
}

fn token<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let raw = <&RawValue>::deserialize(deserializer)?;
    // A character needs at most two escaped UTF-16 code units in JSON.
    if raw.get().len() > super::MAX_TOKEN_CHARS * 12 + 2 {
        return Err(D::Error::custom("adapter token too large"));
    }
    let text: String = serde_json::from_str(raw.get()).map_err(D::Error::custom)?;
    if text.is_empty() || text.chars().count() > super::MAX_TOKEN_CHARS || text.chars().any(char::is_control) {
        return Err(D::Error::custom("invalid adapter token"));
    }
    Ok(text)
}

fn entries<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<&'de RawValue>, D::Error> {
    struct Entries;
    impl<'de> Visitor<'de> for Entries {
        type Value = Vec<&'de RawValue>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a bounded array of adapter entries")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = sequence.next_element_seed(Entry {
                allowed: entries.len() < crate::MAX_LIBRARY_RECORDS,
            })? {
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Entries)
}

struct Entry {
    allowed: bool,
}

impl<'de> DeserializeSeed<'de> for Entry {
    type Value = &'de RawValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        // SeqAccess calls this only when an entry exists; refuse the first
        // excess entry before scanning it or growing the collection.
        if !self.allowed {
            return Err(D::Error::custom("too many adapter entries"));
        }
        let raw = <&RawValue>::deserialize(deserializer)?;
        if raw.get().len() > crate::MAX_RECORD_BYTES {
            return Err(D::Error::custom("adapter entry too large"));
        }
        Ok(raw)
    }
}

fn skipped<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<AdapterSkip>, D::Error> {
    entries(deserializer)?
        .into_iter()
        .map(|raw| serde_json::from_str(raw.get()).map_err(D::Error::custom))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_record_limit_itself_is_accepted_and_vendor_payloads_stay_raw() {
        let mut text = String::from(r#"{"ibis":1,"read":{"scope":"s","revision":"r","records":["#);
        text.push_str(&"0,".repeat(crate::MAX_LIBRARY_RECORDS - 1));
        text.push_str("0]}}");
        let response = decode(text.as_bytes()).unwrap();
        assert_eq!(response.read.unwrap().records.len(), crate::MAX_LIBRARY_RECORDS);

        let nested = format!(
            r#"{{"ibis":1,"read":{{"scope":"s","revision":"r","records":[{{"vendor":{}0{}}}]}}}}"#,
            "[".repeat(1000),
            "]".repeat(1000)
        );
        assert!(
            decode(nested.as_bytes()).is_ok(),
            "unknown nested values need no Value tree"
        );
    }

    #[test]
    fn record_count_is_refused_during_deserialization_before_the_next_payload() {
        let mut text = String::from(r#"{"ibis":1,"read":{"scope":"s","revision":"r","records":["#);
        text.push_str(&"0,".repeat(crate::MAX_LIBRARY_RECORDS));
        // Not valid JSON: the count limit must win before visiting this payload.
        text.push_str("not-json]}}");
        let error = decode(text.as_bytes()).err().unwrap();
        assert!(error.to_string().contains("too many adapter entries"), "{error}");
    }

    #[test]
    fn skipped_count_is_refused_during_deserialization() {
        let mut text = String::from(r#"{"ibis":1,"read":{"scope":"s","revision":"r","records":[],"skipped":["#);
        text.push_str(&r#"{"cause":"unreadable"},"#.repeat(crate::MAX_LIBRARY_RECORDS));
        text.push_str("not-json]}}");
        let error = decode(text.as_bytes()).err().unwrap();
        assert!(error.to_string().contains("too many adapter entries"), "{error}");
    }

    #[test]
    fn oversized_nested_records_and_skips_are_refused_before_materializing_values() {
        for field in ["records", "skipped"] {
            let records = if field == "skipped" { r#""records":[],"# } else { "" };
            let mut text =
                format!(r#"{{"ibis":1,"read":{{"scope":"s","revision":"r",{records}"{field}":[{{"nested":["#);
            // Over 1 MiB of tiny values would expand substantially in a Value tree.
            text.push_str(&"0,".repeat(512 * 1024));
            text.push_str("0]}]}}");
            let error = decode(text.as_bytes()).err().unwrap();
            assert!(
                error.to_string().contains("adapter entry too large"),
                "{field}: {error}"
            );
        }
    }
}
