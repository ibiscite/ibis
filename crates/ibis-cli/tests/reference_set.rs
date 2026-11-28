//! The reference set's contract (docs/reference-set.md), on the invented set in
//! fixtures/reference-set: it is in the layout the spec describes, and Ibis,
//! reading it as a library, writes it again byte for byte.
use ibis_engine::Engine;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The layout docs/reference-set.md describes: `id` and `citation-key`
/// first, every other name in code-point order, two-space indentation.
fn canonical(records: &[Value]) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    struct Record<'a> {
        id: &'a Value,
        #[serde(rename = "citation-key", skip_serializing_if = "Option::is_none")]
        key: Option<&'a Value>,
        // A JSON object's fields, nested ones too, serialize in name order.
        #[serde(flatten)]
        fields: BTreeMap<&'a str, &'a Value>,
    }
    let records: Vec<Record> = records
        .iter()
        .map(|record| Record {
            id: &record["id"],
            key: record.get("citation-key"),
            fields: record
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(name, _)| !matches!(name.as_str(), "id" | "citation-key"))
                .map(|(name, value)| (name.as_str(), value))
                .collect(),
        })
        .collect();
    Ok(serde_json::to_string_pretty(&records)? + "\n")
}

fn ibis(arguments: &[&str]) -> Result<(u8, String), Box<dyn std::error::Error>> {
    let engine = Engine::new().map_err(|error| format!("{error:?}"))?;
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        engine,
        arguments.iter().map(Into::into).collect(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    Ok((code, String::from_utf8(output)?))
}

#[test]
fn the_invented_set_reads_as_a_library_and_writes_back_unchanged() -> TestResult {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/reference-set/references.json");
    let text = std::fs::read_to_string(&path)?;
    let records: Vec<Value> = serde_json::from_str(&text)?;
    assert_eq!(canonical(&records)?, text, "the canonical layout");
    let file = path.to_str().ok_or("path")?;
    let keys: Vec<&str> = records
        .iter()
        .map(|record| record["citation-key"].as_str().ok_or("key"))
        .collect::<Result<_, _>>()?;
    for record in &records {
        assert_eq!(record["id"], record["citation-key"], "id equals the key");
    }
    let mut cite = vec!["ibis", "cite"];
    cite.extend(&keys);
    cite.extend(["--file", file, "--format", "csl-json"]);
    let (code, written) = ibis(&cite)?;
    assert_eq!(code, 0);
    assert_eq!(written, text, "Ibis writes the set byte for byte");
    Ok(())
}
