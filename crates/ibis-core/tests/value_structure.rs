//! Final expanded values must stay inside their own field during parsing.

use ibis_core::bibtex::load_str;
use ibis_core::model::WarningKind;

#[test]
fn unbalanced_day_is_reported_before_date_normalization() {
    for source in [
        r#"@article(k,title={Safe},year=2024,month=3,day="x}")"#,
        "@string(bad=\"x}\")\n@xdata(p,year=2024,month=3,day=bad)\n@article{k,title={Safe},xdata={p}}",
    ] {
        let loaded = load_str("synthetic.bib", source);
        assert_eq!(loaded.references.len(), 1);
        assert_eq!(loaded.references[0].title.as_deref(), Some("Safe"));
        assert_eq!(loaded.references[0].year(), Some(2024));
        assert_eq!(loaded.references[0].issued.and_then(|date| date.start.month), Some(3));
        assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
        assert_eq!(loaded.warnings[0].kind, WarningKind::Field);
        assert_eq!(loaded.warnings[0].key.as_deref(), Some("k"));
        assert!(
            loaded.warnings[0]
                .message
                .ends_with("field `day` has unbalanced braces; it is omitted")
        );
    }
}

#[test]
fn cached_malformed_parents_and_macros_keep_each_consumers_warning() {
    for (definition, fields) in [
        (r#"@xdata(p,title="x}",publisher={Synthetic Press})"#, "xdata={p}"),
        (r#"@string(bad="x}")"#, "title=bad,publisher={Synthetic Press}"),
    ] {
        let source = format!(
            "{definition}\n@article(a,{fields},year=2024)\n@article(b,{fields},year=2024)\n\
             @string(open={{{{\\}}}})\n@string(close={{\\{{}}}})\n\
             @article{{safe,title={{Literal \\{{braces\\}}}},note=open # close,year=2025}}"
        );
        let loaded = load_str("synthetic.bib", &source);
        assert_eq!(loaded.references.len(), 3, "{:?}", loaded.warnings);
        assert_eq!(loaded.warnings.len(), 2);
        for (index, key, line) in [(0, "a", 2), (1, "b", 3)] {
            let reference = &loaded.references[index];
            assert!(reference.title.is_none());
            assert!(reference.authors.is_empty());
            assert_eq!(reference.year(), Some(2024));
            assert_eq!(reference.variables["publisher"], "Synthetic Press");
            let warning = &loaded.warnings[index];
            assert_eq!(warning.kind, WarningKind::Field);
            assert_eq!(warning.key.as_deref(), Some(key));
            assert_eq!(warning.line, Some(line));
            assert_eq!(warning.message, "field `title` has unbalanced braces; it is omitted");
        }
        assert_eq!(loaded.references[2].title_plain().as_deref(), Some("Literal {braces}"));
        assert_eq!(loaded.references[2].variables["note"], "}{");
    }
}

#[test]
fn unbalanced_values_never_create_metadata_or_hide_safe_data() {
    let loaded = load_str(
        "synthetic.bib",
        include_str!("../../../fixtures/bibtex/unbalanced-values.bib"),
    );
    assert_eq!(loaded.references.len(), 6, "{:?}", loaded.warnings);
    for key in ["paren-quoted", "paren-braced", "brace-braced"] {
        let reference = loaded
            .references
            .iter()
            .find(|r| r.id.native_key == key)
            .unwrap_or_else(|| panic!("missing {key}"));
        assert!(reference.title.is_none(), "{key}");
        assert!(reference.authors.is_empty(), "{key}");
        assert!(reference.issued.is_none(), "{key}");
        assert_eq!(
            reference.variables.get("publisher").map(String::as_str),
            Some("Synthetic Press")
        );
        let warning = loaded
            .warnings
            .iter()
            .find(|w| w.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing warning for {key}"));
        assert_eq!(warning.kind, WarningKind::Field);
        assert_eq!(warning.message, "field `title` has unbalanced braces; it is omitted");
        assert!(warning.line.is_some());
    }
    for key in ["safe-quoted", "safe-braced"] {
        let reference = loaded
            .references
            .iter()
            .find(|r| r.id.native_key == key)
            .unwrap_or_else(|| panic!("missing {key}"));
        assert_eq!(reference.title_plain().as_deref(), Some("Literal {braces}"));
        assert_eq!(reference.year(), Some(2024));
    }
    assert_eq!(
        loaded.references.last().and_then(|r| r.title.as_deref()),
        Some("Retained")
    );
    assert_eq!(loaded.warnings.len(), 4);
    assert!(
        loaded
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::Syntax && w.line == Some(4))
    );
    assert!(ibis_core::library::csl_items(&loaded.references).is_ok());
}

#[test]
fn expanded_and_inherited_values_cannot_escape_known_or_unknown_fields() {
    for field in ["title", "journal", "note", "unknown", "url", "author", "date"] {
        for written in [r#""x}, date = {1999-01-01}, author = {Evil Person}""#, "bad # {}"] {
            let source = format!(
                "@string(bad=\"x}}, date = {{1999-01-01}}, author = {{Evil Person}}\")\n\
                 @xdata(p,{field}={written},publisher={{Synthetic Press}})\n\
                 @article{{k,xdata={{p}}}}\n@article{{neighbor,title={{Retained}}}}"
            );
            let loaded = load_str("synthetic.bib", &source);
            assert_eq!(loaded.references.len(), 2, "{field}: {:?}", loaded.warnings);
            let reference = &loaded.references[0];
            assert!(reference.authors.is_empty(), "{field}");
            assert!(reference.issued.is_none(), "{field}");
            assert!(reference.title.is_none(), "{field}");
            assert_eq!(
                reference.variables.get("publisher").map(String::as_str),
                Some("Synthetic Press")
            );
            assert_eq!(loaded.references[1].title.as_deref(), Some("Retained"));
            assert_eq!(loaded.warnings.len(), 1, "{field}: {:?}", loaded.warnings);
            assert_eq!(loaded.warnings[0].kind, WarningKind::Field);
            assert_eq!(loaded.warnings[0].key.as_deref(), Some("k"));
            assert_eq!(loaded.warnings[0].line, Some(3));
            assert_eq!(
                loaded.warnings[0].message,
                format!(
                    "{}field `{field}` has unbalanced braces; it is omitted",
                    if field == "date" { "in inherited `p`: " } else { "" }
                )
            );
        }
    }
}
