use ibis_core::cite::{CiteItem, Locator, LocatorLabel};
use ibis_core::zotero::{Library, load_items};
use serde_json::json;

#[path = "cite_render.rs"]
mod key_output;

/// Recognize only CSL's supported markup and three escaped text characters.
/// Decode entities once, retaining allowed tags so Zotero text can round-trip.
fn checked_rich_text(text: &str) -> String {
    let mut decoded = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if rest.starts_with('<') {
            let end = rest.find('>').unwrap_or_else(|| panic!("unterminated rich-text tag")) + 1;
            let tag = &rest[..end];
            assert!(
                matches!(
                    tag,
                    "<i>"
                        | "</i>"
                        | "<b>"
                        | "</b>"
                        | "<sub>"
                        | "</sub>"
                        | "<sup>"
                        | "</sup>"
                        | "<span class=\"nocase\">"
                        | "<span style=\"font-variant:small-caps;\">"
                        | "</span>"
                ),
                "unsafe CSL markup: {tag}"
            );
            decoded.push_str(tag);
            rest = &rest[end..];
        } else if rest.starts_with('&') {
            let end = rest
                .find(';')
                .unwrap_or_else(|| panic!("unterminated rich-text entity"))
                + 1;
            decoded.push(match &rest[..end] {
                "&amp;" => '&',
                "&lt;" => '<',
                "&gt;" => '>',
                other => panic!("unsafe CSL entity: {other}"),
            });
            rest = &rest[end..];
        } else {
            let c = rest.chars().next().unwrap_or_else(|| panic!("nonempty text"));
            assert_ne!(c, '>', "unescaped rich-text angle bracket");
            decoded.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    decoded
}

pub fn check(data: &[u8]) {
    key_output::check(data);
    let input = String::from_utf8_lossy(data);
    // Raw keys exercise rejection. A safe key and control-free notes also
    // force every successful renderer path, even for arbitrary invalid bytes.
    let note: String = input.chars().filter(|c| !c.is_control()).collect();
    let safe = CiteItem {
        key: "synthetic2026".into(),
        prefix: Some(note.clone()),
        suffix: Some(note.clone()),
        locator: Some(Locator {
            label: LocatorLabel::Page,
            value: note.clone(),
        }),
        suppress_author: data.first().is_some_and(|b| b & 1 != 0),
    };
    key_output::check_items(&[]);
    key_output::check_items(std::slice::from_ref(&safe));
    let key: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control() && !",{}".contains(*c))
        .collect();
    let arbitrary = CiteItem {
        key: if key.is_empty() { "fallback".into() } else { key },
        locator: Some(Locator {
            label: LocatorLabel::Chapter,
            value: note,
        }),
        ..safe.clone()
    };
    key_output::check_items(&[safe, arbitrary]);

    // The real mapper accepts arbitrary title text, including malformed tags.
    // A sentinel keeps even whitespace-only inputs on the export path.
    let title = format!("Synthetic {input} end");
    let page = [json!({"key": "FUZZTEXT", "data": {"itemType": "book"},
        "csljson": {"type": "book", "title": title, "container-title": title}})];
    let loaded = load_items(Library::User, &page);
    assert_eq!(loaded.references.len(), 1);
    let csl = loaded.references[0].to_csl_json();
    for field in ["title", "container-title"] {
        assert_eq!(
            checked_rich_text(csl[field].as_str().unwrap_or_else(|| panic!("rich text retained"))),
            title
        );
    }
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&csl.to_string()).ok(),
        Some(csl)
    );

    // Hostile BibTeX may reject fields; every field it does export must escape
    // literal markup, including text recovered by the tolerant fallback.
    let bib = format!("@book{{synthetic,title={{{input}}},booktitle={{{input}}}}}");
    for reference in ibis_core::bibtex::load_str("synthetic.bib", &bib).references {
        let csl = reference.to_csl_json();
        for field in ["title", "title-short", "container-title"] {
            if let Some(text) = csl[field].as_str() {
                checked_rich_text(text);
            }
        }
    }
}
