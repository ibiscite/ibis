//! `check`: a manuscript's citations against the user's library and,
//! when sent, a reference set's records (docs/reference-set.md). The agent
//! sends the text; the server reads no file for it, and the manuscript is
//! parsed only in the document child (decision 0060).
use super::*;
use ibis_core::contract::TextKind;
use ibis_engine::{CheckResult, DocumentChecked};

/// Findings one answer lists at most; `more` counts the rest.
pub const MAX_FINDINGS: usize = 200;

pub(super) fn check_input() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {"type": "string", "minLength": 1, "description": "The manuscript's whole text, as saved."},
            "kind": {
                "type": "string",
                "enum": ["markdown", "latex", "typst"],
                "description": "markdown for Markdown, Quarto and R Markdown; latex for BibTeX, natbib or BibLaTeX commands; typst."
            },
            "set": {
                "type": "array",
                "items": {"type": "object"},
                "description": "A reference set: the CSL-JSON records the manuscript was written against, as its references.json holds them."
            }
        },
        "required": ["text", "kind"],
        "additionalProperties": false
    })
}

pub(super) fn check_output() -> Value {
    json!({
        "type": "object",
        "properties": {
            "outcome": {"type": "string", "enum": ["checked", "refused"]},
            "settled": {"type": "boolean", "description": "Nothing stops the manuscript building from these libraries."},
            "citations": {"type": "integer"},
            "keys": {"type": "integer"},
            "complete": {"type": "boolean", "description": "The whole text was read; a text read in part never settles."},
            "findings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "place": {"type": "object", "properties": {"line": {"type": "integer"}, "column": {"type": "integer"}}},
                        "key": {"type": "string"},
                        "current": {"type": "string", "description": "For a renamed key, the key to use now."},
                        "problem": {"type": "string"}
                    },
                    "required": ["problem"]
                }
            },
            "more": {"type": "integer", "description": "Findings left out of this answer."},
            "file": {"type": "string", "description": "For a refusal: document or set."},
            "reason": {"type": "string", "description": "For a refusal: why it was not read."},
            "source_health": {"type": "object"},
            "provenance": {"type": "string"}
        },
        "required": ["outcome", "settled"]
    })
}

pub(super) fn check(
    engine: &Engine,
    _: &Formats,
    arguments: &Map<String, Value>,
    cancel: &Cancellation,
) -> Result<Value, Refusal> {
    let text = arguments
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| Refusal::bad(ToolProblem::Text, &[("name", "text")]))?;
    let kind = match arguments.get("kind").and_then(Value::as_str) {
        Some("markdown") => TextKind::Markdown,
        Some("latex") => TextKind::Latex,
        Some("typst") => TextKind::Typst,
        _ => return Err(Refusal::bad(ToolProblem::ManuscriptKind, &[])),
    };
    let set = match arguments.get("set") {
        None => None,
        Some(Value::Array(records)) if records.iter().all(Value::is_object) => Some(records.clone()),
        Some(_) => return Err(Refusal::bad(ToolProblem::Set, &[])),
    };
    let result = engine.check_text(kind, text, set, cancel)?;
    let settled = result.value.settled();
    let mut answer = match result.value {
        CheckResult::Refused { file, reason } => {
            json!({"outcome": "refused", "settled": false, "file": file, "reason": reason})
        }
        CheckResult::Checked(DocumentChecked { read }) => {
            let more = read.findings.len().saturating_sub(MAX_FINDINGS);
            let mut answer = json!({
                "outcome": "checked",
                "settled": settled,
                "citations": read.citations,
                "keys": read.keys,
                "complete": read.complete,
                "findings": read.findings.iter().take(MAX_FINDINGS).collect::<Vec<_>>(),
            });
            if more > 0 {
                answer["more"] = json!(more);
            }
            answer
        }
    };
    answer["source_health"] = json!(result.source_health);
    // A renamed key's current name comes from the library.
    answer["provenance"] = json!(PROVENANCE);
    Ok(answer)
}
