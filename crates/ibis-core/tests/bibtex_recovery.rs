//! Recovery must retain good records without changing complete multiline values.

use ibis_core::{bibtex::load_str, model::WarningKind};

#[test]
fn incomplete_entries_recover_the_following_record_with_line_numbers() {
    for open in ['{', '('] {
        let text = format!("@article{open}first,\n  @article{open}second,\n\t@book{{kept,title={{Retained}}}}\n");
        let loaded = load_str("synthetic.bib", &text);
        assert_eq!(loaded.references.len(), 1);
        assert_eq!(loaded.references[0].id.native_key, "kept");
        assert_eq!(loaded.references[0].title.as_deref(), Some("Retained"));
        assert_eq!(loaded.warnings.len(), 2);
        for (index, warning) in loaded.warnings.iter().enumerate() {
            assert_eq!(warning.kind, WarningKind::Syntax);
            assert_eq!(warning.line, Some(index + 1));
            assert!(warning.message.contains("not closed"));
        }
    }
}

#[test]
fn entry_looking_lines_inside_complete_values_are_preserved() {
    for (open, close) in [('{', '}'), ('(', ')')] {
        for (value_open, value_close) in [('{', '}'), ('"', '"')] {
            let outer = format!(
                "@article{open}outer,title={value_open}Before\n  @book{{inside,title={{Synthetic}}}}\nAfter{value_close}{close}"
            );
            let text = format!("@book{{broken,\n{outer}\n@book{{kept,title={{Retained}}}}\n");
            let loaded = load_str("synthetic.bib", &text);
            assert_eq!(loaded.references.len(), 2, "{text}: {:?}", loaded.warnings);
            assert_eq!(loaded.references[0].id.native_key, "outer");
            let title = loaded.references[0].title.as_deref().unwrap_or_default();
            assert!(title.to_lowercase().contains("after"));
            assert_eq!(loaded.references[0].raw.as_deref(), Some(outer.as_str()));
            assert_eq!(loaded.references[1].id.native_key, "kept");
            assert_eq!(loaded.warnings.len(), 1, "{text}: {:?}", loaded.warnings);
        }
    }
}

#[test]
fn quotes_parentheses_escapes_and_comments_keep_their_structure() {
    let text = "@book{broken,\n@book(outer, title=\"A (B) C \\\"u and 100%\",\n% ignored ) } { \"\n year={2024})\n@book{kept,title={Retained}}\n";
    let loaded = load_str("synthetic.bib", text);
    assert_eq!(loaded.references.len(), 2, "{:?}", loaded.warnings);
    assert_eq!(loaded.references[0].year(), Some(2024));
    let title = loaded.references[0].title.as_deref().unwrap_or_default();
    assert!(title.to_lowercase().contains("a (b) c"));
    assert_eq!(loaded.references[1].id.native_key, "kept");
    assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
}
