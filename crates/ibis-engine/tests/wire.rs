use ibis_engine::wire::encode;
use serde_json::{Value, json};

#[test]
fn object_order_is_canonical_and_citation_strings_remain_verbatim() -> Result<(), Box<dyn std::error::Error>> {
    let first: Value = serde_json::from_str(r#"{"z":[{"β":2,"a":1}],"a":"[@Example2026, a -- \"b\" ...]"}"#)?;
    let mut map = serde_json::Map::new();
    map.insert("a".into(), first["a"].clone());
    map.insert("z".into(), first["z"].clone());
    let second = Value::Object(map);
    let bytes = encode(&first, 1024)?;
    assert_eq!(bytes, encode(&second, 1024)?);
    assert!(std::str::from_utf8(&bytes)?.starts_with("{\"a\":"));
    assert!(std::str::from_utf8(&bytes)?.contains("\"a\":1,\"β\":2"));
    assert_eq!(serde_json::from_slice::<Value>(&bytes)?["a"], first["a"]);
    assert!(encode(&json!({"a":"x".repeat(1024)}), 1024).is_err());
    Ok(())
}
