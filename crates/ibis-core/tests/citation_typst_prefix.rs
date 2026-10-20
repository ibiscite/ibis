//! Synthetic notes must stay plain text when pasted into Typst source.

use ibis_core::cite::{CiteItem, KeyFormat, render};

#[test]
fn typst_prefix_escapes_heading_and_list_markers() {
    // Pandoc 3.1.11.1's Typst reader parses the unescaped forms as a Header,
    // OrderedList, BulletList and DefinitionList, respectively. The prefix is
    // user text, not a request to turn the citation into one of those blocks.
    for (prefix, escaped) in [
        ("= synthetic note", "\\= synthetic note"),
        ("+ synthetic note", "\\+ synthetic note"),
        ("- synthetic note", "\\- synthetic note"),
        ("/ synthetic: note", "\\/ synthetic: note"),
        ("  - synthetic note", "  \\- synthetic note"),
        ("1. synthetic note", "1\\. synthetic note"),
        ("note\u{2028}= heading", "note\u{2028}\\= heading"),
        ("note\u{2029}- item", "note\u{2029}\\- item"),
    ] {
        let item = CiteItem {
            key: "synthetic2024".to_owned(),
            prefix: Some(prefix.to_owned()),
            ..CiteItem::default()
        };
        let rendered = render(KeyFormat::Typst, &[item]).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(rendered.text, format!("{escaped} @synthetic2024"));
    }
}

#[test]
fn typst_supplements_escape_block_markers_without_changing_inline_punctuation() {
    for (note, escaped) in [
        ("= note", "\\= note"),
        ("+ note", "\\+ note"),
        ("- note", "\\- note"),
        ("/ term: note", "\\/ term: note"),
        ("12. note", "12\\. note"),
        ("pre-existing and/or 2024-10-19", "pre-existing and/or 2024-10-19"),
    ] {
        let item = CiteItem {
            key: "synthetic2024".to_owned(),
            suffix: Some(note.to_owned()),
            ..CiteItem::default()
        };
        let rendered = render(KeyFormat::Typst, &[item]).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(rendered.text, format!("@synthetic2024[{escaped}]"));
    }
}
