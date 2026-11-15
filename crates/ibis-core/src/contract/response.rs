//! Shared request values read as evidence in responses may gain additive fields.
//! Project onto their canonical schema before decoding; direct requests still
//! use their strict deserializers, and known values keep all their validation.
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use serde_json::Value;
use std::sync::OnceLock;

pub(crate) trait Additive: DeserializeOwned + JsonSchema {
    fn response_schema() -> &'static Value;
}

// Each concrete type owns its schema; large snapshots never rebuild it per id.
macro_rules! schemas {
    ($($ty:ty),* $(,)?) => {$(
        impl Additive for $ty {
            fn response_schema() -> &'static Value {
                static SCHEMA: OnceLock<Value> = OnceLock::new();
                SCHEMA.get_or_init(|| schemars::schema_for!($ty).to_value())
            }
        }
    )*};
}
schemas!(
    super::CitationRequest,
    super::ItemRef,
    Option<super::ItemRef>,
    Vec<super::ItemRef>,
    super::OutputSpec,
    crate::destination::AppId,
);

pub(crate) fn additive<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Additive,
{
    let mut value = Value::deserialize(deserializer)?;
    let schema = T::response_schema();
    known_fields(&mut value, schema, schema);
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

fn resolve<'a>(schema: &'a Value, root: &'a Value) -> &'a Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix('#'))
        .and_then(|pointer| root.pointer(pointer))
        .unwrap_or(schema)
}

fn fits(value: &Value, schema: &Value, root: &Value) -> bool {
    let schema = resolve(schema, root);
    let shape = |kind: &str| match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        _ => false,
    };
    let shape_matches = match schema.get("type") {
        Some(Value::String(kind)) => shape(kind),
        Some(Value::Array(kinds)) => kinds.iter().filter_map(Value::as_str).any(shape),
        _ => true,
    };
    shape_matches
        && schema
            .get("properties")
            .and_then(Value::as_object)
            .is_none_or(|fields| {
                fields
                    .iter()
                    .all(|(name, field)| field.get("const").is_none_or(|tag| value.get(name) == Some(tag)))
            })
}

fn known_fields(value: &mut Value, schema: &Value, root: &Value) {
    let schema = resolve(schema, root);
    if let Some(variants) = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(Value::as_array)
    {
        if let Some(variant) = variants.iter().find(|variant| fits(value, variant, root)) {
            known_fields(value, variant, root);
        }
        return;
    }
    match value {
        Value::Object(fields) => {
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                    fields.retain(|name, _| properties.contains_key(name));
                }
                for (name, value) in fields {
                    if let Some(field) = properties.get(name) {
                        known_fields(value, field, root);
                    }
                }
            }
        }
        Value::Array(values) => {
            if let Some(items) = schema.get("items") {
                for value in values {
                    known_fields(value, items, root);
                }
            }
        }
        _ => {}
    }
}
