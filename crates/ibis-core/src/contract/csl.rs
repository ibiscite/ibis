//! A record's CSL variables, with name lists held compactly (#1675, #1685).
//!
//! A CSL name is a JSON object, and every `serde_json` object is a `BTreeMap`
//! whose first node reserves eleven slots: about 650 bytes for a two-part name.
//! Names are most of a library's memory (55 of 88 MiB at 10,000 records). Here a
//! name list whose every name has only text parts is kept as [`CompactName`]s,
//! about 100 bytes a name, and written back as exactly the JSON it came from, so
//! serialization, fingerprints, formatting and every front end see no change.
//! A list in any other shape stays as its JSON.
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// A name's parts, in the order a JSON object of them serializes.
const PARTS: [&str; 6] = [
    "dropping-particle",
    "family",
    "given",
    "literal",
    "non-dropping-particle",
    "suffix",
];

/// One name with only text parts, indexed as [`PARTS`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactName([Option<Box<str>>; 6]);

impl CompactName {
    /// The name `value` holds, when it is an object of text parts Ibis knows.
    fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let mut parts: [Option<Box<str>>; 6] = Default::default();
        for (key, part) in object {
            let index = PARTS.iter().position(|known| known == key)?;
            parts[index] = Some(part.as_str()?.into());
        }
        Some(Self(parts))
    }

    /// A part by its CSL name, such as `family`.
    pub fn part(&self, name: &str) -> Option<&str> {
        PARTS
            .iter()
            .position(|known| *known == name)
            .and_then(|index| self.0[index].as_deref())
    }

    /// The name, as search and summaries read names.
    pub fn to_name(&self) -> crate::model::Name {
        let part = |name: &str| self.part(name).map(str::to_owned);
        crate::model::Name {
            family: part("family"),
            given: part("given"),
            literal: part("literal"),
            dropping_particle: part("dropping-particle"),
            non_dropping_particle: part("non-dropping-particle"),
            suffix: part("suffix"),
        }
    }

    fn to_value(&self) -> Value {
        Value::Object(
            PARTS
                .iter()
                .zip(&self.0)
                .filter_map(|(key, part)| Some(((*key).to_owned(), Value::String(part.as_deref()?.to_owned()))))
                .collect::<Map<_, _>>(),
        )
    }
}

impl Serialize for CompactName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.iter().flatten().count()))?;
        for (key, part) in PARTS.iter().zip(&self.0) {
            if let Some(part) = part {
                map.serialize_entry(key, part)?;
            }
        }
        map.end()
    }
}

/// A compact name list, and its JSON once something asks for it as JSON.
#[derive(Debug, Clone)]
pub struct Names {
    names: Box<[CompactName]>,
    json: OnceLock<Value>,
}

impl Names {
    pub fn names(&self) -> &[CompactName] {
        &self.names
    }
    fn json(&self) -> &Value {
        self.json
            .get_or_init(|| Value::Array(self.names.iter().map(CompactName::to_value).collect()))
    }
}

impl PartialEq for Names {
    fn eq(&self, other: &Self) -> bool {
        self.names == other.names
    }
}
impl Eq for Names {}

/// One variable's value as it is held.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Held {
    Json(Value),
    Names(Names),
}

impl Held {
    fn from_value(value: Value) -> Self {
        let names = value
            .as_array()
            .filter(|list| !list.is_empty())
            .and_then(|list| list.iter().map(CompactName::from_value).collect::<Option<Box<[_]>>>());
        match names {
            Some(names) => Self::Names(Names {
                names,
                json: OnceLock::new(),
            }),
            None => Self::Json(value),
        }
    }
    fn json(&self) -> &Value {
        match self {
            Self::Json(value) => value,
            Self::Names(names) => names.json(),
        }
    }
    fn into_json(self) -> Value {
        match self {
            Self::Json(value) => value,
            Self::Names(Names { names, json }) => json
                .into_inner()
                .unwrap_or_else(|| Value::Array(names.iter().map(CompactName::to_value).collect())),
        }
    }
}

impl Serialize for Held {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Json(value) => value.serialize(serializer),
            Self::Names(names) => {
                let mut list = serializer.serialize_seq(Some(names.names.len()))?;
                for name in names.names.iter() {
                    list.serialize_element(name)?;
                }
                list.end()
            }
        }
    }
}

/// A variable as it is held: JSON, or a compact name list.
#[derive(Debug, Clone, Copy)]
pub enum CslRef<'a> {
    Json(&'a Value),
    Names(&'a [CompactName]),
}

impl Serialize for CslRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Json(value) => value.serialize(serializer),
            Self::Names(names) => {
                let mut list = serializer.serialize_seq(Some(names.len()))?;
                for name in names.iter() {
                    list.serialize_element(name)?;
                }
                list.end()
            }
        }
    }
}

/// A record's CSL variables by name, in name order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Csl(BTreeMap<String, Held>);

impl Csl {
    pub fn new() -> Self {
        Self::default()
    }

    /// A variable as JSON. A name list's JSON is built when first asked for and
    /// kept; bulk readers use [`Csl::entries`] or [`Csl::names`] instead.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key).map(Held::json)
    }

    /// A name list's names, when it is held compactly.
    pub fn names(&self, key: &str) -> Option<&[CompactName]> {
        match self.0.get(key)? {
            Held::Names(names) => Some(names.names()),
            Held::Json(_) => None,
        }
    }

    /// Every variable as it is held, without building JSON for name lists.
    pub fn entries(&self) -> impl Iterator<Item = (&str, CslRef<'_>)> {
        self.0.iter().map(|(key, held)| {
            (
                key.as_str(),
                match held {
                    Held::Json(value) => CslRef::Json(value),
                    Held::Names(names) => CslRef::Names(names.names()),
                },
            )
        })
    }

    /// Every variable as JSON; name lists build theirs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.0.iter().map(|(key, held)| (key, held.json()))
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.0.values().map(Held::json)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        self.0.insert(key, Held::from_value(value)).map(Held::into_json)
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.0.remove(key).map(Held::into_json)
    }

    /// A variable to change in place; a name list becomes JSON again and is
    /// compacted next time the record is built.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        let held = self.0.get_mut(key)?;
        if let Held::Names(_) = held {
            let value = std::mem::replace(held, Held::Json(Value::Null)).into_json();
            *held = Held::Json(value);
        }
        match held {
            Held::Json(value) => Some(value),
            Held::Names(_) => None,
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&str, &Value) -> bool) {
        self.0.retain(|key, held| keep(key, held.json()));
    }

    /// The variables as a JSON object, as formatting and agents read them. It
    /// builds name lists' JSON afresh and keeps none of it.
    pub fn to_map(&self) -> Map<String, Value> {
        self.0
            .iter()
            .map(|(key, held)| {
                let value = match held {
                    Held::Json(value) => value.clone(),
                    Held::Names(names) => match names.json.get() {
                        Some(value) => value.clone(),
                        None => Value::Array(names.names.iter().map(CompactName::to_value).collect()),
                    },
                };
                (key.clone(), value)
            })
            .collect()
    }

    /// The values held as JSON, leaving out compact name lists (which hold
    /// only short text), so bulk checks build no JSON.
    pub fn json_values(&self) -> impl Iterator<Item = &Value> {
        self.0.values().filter_map(|held| match held {
            Held::Json(value) => Some(value),
            Held::Names(_) => None,
        })
    }

    /// The values held as JSON, by value; compact name lists are dropped.
    pub fn into_json_values(self) -> impl Iterator<Item = Value> {
        self.0.into_values().filter_map(|held| match held {
            Held::Json(value) => Some(value),
            Held::Names(_) => None,
        })
    }
}

impl std::ops::Index<&str> for Csl {
    type Output = Value;
    fn index(&self, key: &str) -> &Value {
        static NULL: Value = Value::Null;
        self.get(key).unwrap_or(&NULL)
    }
}

impl FromIterator<(String, Value)> for Csl {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(variables: I) -> Self {
        Self(
            variables
                .into_iter()
                .map(|(key, value)| (key, Held::from_value(value)))
                .collect(),
        )
    }
}

impl From<BTreeMap<String, Value>> for Csl {
    fn from(variables: BTreeMap<String, Value>) -> Self {
        variables.into_iter().collect()
    }
}

impl From<Map<String, Value>> for Csl {
    fn from(variables: Map<String, Value>) -> Self {
        variables.into_iter().collect()
    }
}

impl<'a> IntoIterator for &'a Csl {
    type Item = (&'a String, &'a Value);
    type IntoIter = Box<dyn Iterator<Item = (&'a String, &'a Value)> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl Serialize for Csl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, held) in &self.0 {
            map.serialize_entry(key, held)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Csl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BTreeMap::<String, Value>::deserialize(deserializer).map(Self::from)
    }
}

impl JsonSchema for Csl {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        <BTreeMap<String, Value>>::schema_name()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <BTreeMap<String, Value>>::json_schema(generator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn csl(value: Value) -> Csl {
        serde_json::from_value(value).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn names_are_held_compactly_and_read_back_as_the_same_json() {
        let source = json!({
            "author": [{"given": "Ana", "family": "García"}, {"literal": "Invented Group"}],
            "editor": [{"family": "Moss", "non-dropping-particle": "van", "suffix": "Jr."}],
            "title": "Synthetic tides",
            "issued": {"date-parts": [[2021]]},
        });
        let held = csl(source.clone());
        assert!(held.names("author").is_some() && held.names("editor").is_some());
        assert_eq!(held.names("title"), None);
        assert_eq!(serde_json::to_value(&held).unwrap_or_default(), source);
        assert_eq!(
            serde_json::to_string(&held).unwrap_or_default(),
            serde_json::to_string(&source).unwrap_or_default()
        );
        assert_eq!(held.get("author"), source.get("author"));
        assert_eq!(
            held.names("author").and_then(|names| names[0].part("family")),
            Some("García")
        );
        assert_eq!(Value::Object(held.to_map()), source);
    }

    #[test]
    fn a_list_in_any_other_shape_stays_as_its_json() {
        for list in [
            json!([{"family": "García", "parse-names": true}]),
            json!([{"family": 7}]),
            json!(["García"]),
            json!([]),
            json!([{"family": "García"}, "Okafor"]),
        ] {
            let held = csl(json!({"author": list}));
            assert_eq!(held.names("author"), None, "{list}");
            assert_eq!(held.get("author"), Some(&list));
            assert_eq!(serde_json::to_value(&held).unwrap_or_default(), json!({"author": list}));
        }
    }

    #[test]
    fn changing_a_name_list_in_place_keeps_its_meaning() {
        let mut held = csl(json!({"author": [{"family": "García"}]}));
        if let Some(Value::Array(names)) = held.get_mut("author") {
            names.push(json!({"family": "Okafor"}));
        }
        assert_eq!(
            held.get("author"),
            Some(&json!([{"family": "García"}, {"family": "Okafor"}]))
        );
        assert_eq!(
            held.remove("author"),
            Some(json!([{"family": "García"}, {"family": "Okafor"}]))
        );
        assert!(held.is_empty());
    }
}
