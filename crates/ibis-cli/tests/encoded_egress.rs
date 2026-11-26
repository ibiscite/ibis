//! Invented privacy attacks through the real CLI and MCP stdio server.
#![allow(clippy::unwrap_used)]
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

const FORMATS: &[&str] = &[
    "bibtex",
    "csl-json",
    "doi-links",
    "apa",
    "chicago-author-date",
    "harvard-cite-them-right",
    "ieee",
    "modern-language-association",
];
const PATHS: &[&str] = &[
    "%2FUsers%2Finvented%2Fsecret.pdf",
    "%252FUsers%252Finvented%252Fsecret.pdf",
    "&#37;2FUsers&#37;2Finvented&#37;2Fsecret.pdf",
    "%26%2347%3BUsers%26%2347%3Binvented%26%2347%3Bsecret.pdf",
    "&amp;#47;Users/invented/secret.pdf",
    "<i>%2FUsers%2F</i><b>invented%2Fsecret.pdf</b>",
];
/// Paths opened by marks other than value delimiters, or by HTML-only spellings.
const WRAPPED: &[&str] = &[
    "&#X2F;Users&#X2F;invented&#X2F;secret.pdf",
    "&#x00002F;Users&#x00002F;invented&#x00002F;secret.pdf",
    "\u{201c}/Users/invented/secret.pdf\u{201d}",
    "\u{ab}/Users/invented/secret.pdf\u{bb}",
    "\u{2014}/Users/invented/secret.pdf",
    "*/Users/invented/secret.pdf*",
    "?/Users/invented/secret.pdf",
    "\u{200b}/Users/invented/secret.pdf",
    "Saved--/Users/invented/secret.pdf",
    "#/Users/invented/secret.pdf",
];

fn path_records(paths: &[&str]) -> Value {
    Value::Array(
        paths
            .iter()
            .enumerate()
            .map(|(i, path)| {
                json!({
                    "id":format!("r{i}"), "citation-key":format!("r{i}"), "type":"book",
                    "title":path, "author":[{"family":path}], "issued":{"literal":path},
                    "abstract":path, "note":format!("archive: {path}"),
                    "publisher":"Invented Press", "DOI":format!("10.1000/x;{path}")
                })
            })
            .collect(),
    )
}

fn doi_records() -> Value {
    json!([
        {"id":"r0", "citation-key":"r0", "type":"book", "title":"Invented atlas",
         "DOI":"10.1000//Users/invented/secret.pdf"},
        {"id":"r1", "citation-key":"r1", "type":"book", "title":"Invented atlas",
         "URL":"https://doi.org/&#47;Users/invented/secret.pdf"},
        {"id":"r2", "citation-key":"r2", "type":"book", "title":"Invented atlas",
         "DOI":"10.1000/%252FUsers/invented/secret.pdf"},
        {"id":"r3", "citation-key":"r3", "type":"book", "title":"Invented atlas",
         "URL":"https://dx.doi.org/%2FUsers/invented/secret.pdf"}
    ])
}

fn exports_are_private(records: Value, formatted_refusal: Option<&str>) {
    let mut leaks = Vec::new();
    for format in FORMATS {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(["cite", "--records", "--format", format, "--json"])
            .args(if ["bibtex", "csl-json", "doi-links"].contains(format) {
                vec![]
            } else {
                vec!["--bibliography"]
            })
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(records.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            matches!(output.status.code(), Some(0 | 1)),
            "{format}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reply: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(reply["ok"], true, "{format}: {reply}");
        let result = &reply["data"];
        if result.to_string().contains("secret") {
            leaks.push(format!("{format}: {}", result["text"]));
        }
        if *format == "doi-links" {
            if result["text"] != "" {
                leaks.push(format!("DOI not refused: {result}"));
            }
            for outcome in result["outcomes"].as_array().unwrap() {
                if outcome["error"] != "missing-doi" {
                    leaks.push(format!("wrong refusal: {outcome}"));
                }
            }
            assert_eq!(
                result["outcomes"].as_array().unwrap().len(),
                records.as_array().unwrap().len()
            );
        } else if !["bibtex", "csl-json"].contains(format)
            && let Some(error) = formatted_refusal
        {
            // Filtering every title and creator leaves only unrelated metadata.
            assert_eq!(result["text"], "", "{format}: {result}");
            let outcomes = result["outcomes"].as_array().unwrap();
            assert_eq!(outcomes.len(), records.as_array().unwrap().len());
            for (item, outcome) in outcomes.iter().enumerate() {
                assert_eq!(outcome["item"], item, "{format}: {outcome}");
                assert_eq!(outcome["outcome"], "refused", "{format}: {outcome}");
                assert_eq!(outcome["error"], error, "{format}: {outcome}");
            }
        } else {
            assert!(
                result["text"].as_str().unwrap().contains("Invented"),
                "{format}: {result}"
            );
        }
    }
    assert!(leaks.is_empty(), "{}", leaks.join("\n"));
}

fn mcp_is_private(records: Value) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.json");
    std::fs::write(&path, records.to_string()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibis"))
        .args(["mcp", "--file"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let calls = std::iter::once(("search", json!({"query":"invented"})))
        .chain((0..records.as_array().unwrap().len()).map(|i| ("show", json!({"ref":format!("r{i}")}))));
    for (id, (name, arguments)) in calls.enumerate() {
        let call = json!({"jsonrpc":"2.0", "id":id, "method":"tools/call", "params":{
            "name":name, "arguments":arguments, "_meta":{
                "io.modelcontextprotocol/protocolVersion":ibis_engine::mcp::PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities":{}
            }
        }});
        writeln!(stdin, "{call}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    let replies: Vec<Value> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(replies.len(), records.as_array().unwrap().len() + 1);
    for reply in &replies {
        assert!(reply.get("error").is_none(), "{reply}");
        assert_eq!(reply["result"]["isError"], false, "{reply}");
    }
    assert_eq!(
        replies[0]["result"]["structuredContent"]["hits"]
            .as_array()
            .unwrap()
            .len(),
        records.as_array().unwrap().len()
    );
    assert!(!text.contains("secret"), "{text}");
}

#[test]
fn percent_encoded_paths_never_leave_cli_exports() {
    exports_are_private(path_records(PATHS), Some("missing-details"));
}

#[test]
fn wrapped_paths_never_leave_mcp_search_or_show() {
    // Stay below the server's bounded tool queue.
    for paths in WRAPPED.chunks(6) {
        mcp_is_private(path_records(paths));
    }
}

#[test]
fn doi_shaped_paths_never_leave_cli_exports() {
    exports_are_private(doi_records(), None);
}
