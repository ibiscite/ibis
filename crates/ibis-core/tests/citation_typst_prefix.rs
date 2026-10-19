//! Synthetic notes must stay plain text when pasted into Typst source.

use ibis_core::cite::{CiteItem, KeyFormat, render};

#[test]
#[ignore = "#336: Typst plain-text prefixes become headings and lists"]
fn typst_prefix_escapes_heading_and_list_markers() {
    // Pandoc 3.1.11.1's Typst reader parses the unescaped forms as a Header,
    // OrderedList, BulletList and DefinitionList, respectively. The prefix is
    // user text, not a request to turn the citation into one of those blocks.
    for (prefix, escaped) in [
        ("= synthetic note", "\\= synthetic note"),
        ("+ synthetic note", "\\+ synthetic note"),
        ("- synthetic note", "\\- synthetic note"),
        ("/ synthetic: note", "\\/ synthetic: note"),
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
