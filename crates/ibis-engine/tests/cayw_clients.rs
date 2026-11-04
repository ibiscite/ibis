//! Requests recorded from real Cite as You Write clients reach the picker with
//! the output they asked for (#42; fixtures/cayw/README.md).
use ibis_core::contract::{CitationForm, KeySyntax, OutputSpec};
use ibis_engine::cayw::{self, CaywRequest};
use serde_json::Value;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";
const PORT: u16 = 23_124;

fn recorded(name: &str) -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/cayw")
        .join(name);
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{error}"));
    let fixture: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}"));
    fixture["requests"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|request| request.replace("{port}", &PORT.to_string()))
        .collect()
}

#[test]
fn the_vs_code_citation_picker_gets_each_format_it_offers() {
    let requests = recorded("vscode-zotero-citation-picker.json");
    let key = |syntax, command: Option<&str>| {
        Some(OutputSpec::Key {
            syntax,
            command: command.map(str::to_owned),
        })
    };
    let expected = [
        (key(KeySyntax::Pandoc, Some("bare")), CitationForm::Normal),
        (key(KeySyntax::Pandoc, Some("bare")), CitationForm::Normal),
        (key(KeySyntax::Biblatex, Some("autocite")), CitationForm::Normal),
        (key(KeySyntax::Latex, Some("cite")), CitationForm::Normal),
        (key(KeySyntax::Typst, None), CitationForm::Normal),
        (None, CitationForm::Normal),
        (None, CitationForm::BibliographyOnly),
    ];
    assert_eq!(requests.len(), expected.len());
    for (request, (output, form)) in requests.iter().zip(expected) {
        let Ok(CaywRequest::Pick(pick)) = cayw::parse(request.as_bytes(), PORT, TOKEN) else {
            panic!("refused: {request}");
        };
        let spec = pick.spec("apa");
        assert_eq!(
            (output.unwrap_or(OutputSpec::Csl { style: "apa".into() }), form),
            (spec, pick.form),
            "{request}"
        );
        assert!(!pick.clipboard);
    }
}

#[test]
fn the_same_requests_without_the_token_or_from_a_page_are_refused() {
    for request in recorded("vscode-zotero-citation-picker.json") {
        let untokened = request.replace(&format!("/{TOKEN}"), "");
        assert_eq!(
            cayw::parse(untokened.as_bytes(), PORT, TOKEN),
            Err(cayw::Refusal::Token)
        );
        let from_a_page = request.replacen("\r\n", "\r\nOrigin: https://invented.example\r\n", 1);
        assert_eq!(
            cayw::parse(from_a_page.as_bytes(), PORT, TOKEN),
            Err(cayw::Refusal::Browser)
        );
    }
}
