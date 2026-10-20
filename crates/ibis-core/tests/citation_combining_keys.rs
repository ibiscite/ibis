//! Synthetic citation keys must retain their identity in the target syntax.

use ibis_core::cite::{CiteItem, KeyFormat, render};

#[test]
fn pandoc_quotes_alphabetic_combining_marks_in_keys() {
    // Rust's Unicode Alphabetic property includes these marks and symbols, but
    // Pandoc's bare citation-key parser stops before them. With `[@key]`, the
    // remaining text becomes a suffix and silently cites a different key.
    // Pandoc 3.1.11.1 reads the braced spelling back without changing the key.
    for key in [
        "synthetic\u{0345}2024",
        "synthetic\u{05b0}2024",
        "synthetic\u{093e}2024",
        "synthetic\u{24b6}2024",  // Circled alphabetic symbol, not a mark.
        "synthetic\u{1f130}2024", // Squared alphabetic symbol, not a mark.
    ] {
        let item = CiteItem {
            key: key.to_owned(),
            ..CiteItem::default()
        };
        let rendered = render(KeyFormat::Pandoc, &[item]).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(rendered.text, format!("[@{{{key}}}]"), "key {key:?}");
        assert!(rendered.warnings.is_empty());
    }
}
