//! Equivalent BibTeX spellings must preserve meaning and diagnostics.
//! Duplicate keys/fields and conflicting macro definitions are intentionally
//! excluded: their documented first/last-definition precedence is meaningful.

use ibis_core::bibtex::load_str;
use ibis_core::model::LoadResult;
use proptest::prelude::*;

#[derive(Clone, Debug)]
struct Entry {
    title: String,
    year: u16,
    order: u64,
    fields: [u64; 7],
}

fn libraries() -> impl Strategy<Value = Vec<Entry>> {
    prop::collection::vec(
        (
            prop::sample::select(vec!["Café", "Ελληνικά", "記憶", "বাংলা", "العربية"]),
            "[a-zA-Z]{1,20}",
            1500u16..2100,
            any::<u64>(),
            any::<[u64; 7]>(),
        )
            .prop_map(|(script, text, year, order, fields)| Entry {
                title: format!("{script} {text}"),
                year,
                order,
                fields,
            }),
        1..7,
    )
}

#[derive(Clone, Copy)]
enum Transform {
    None,
    EntryOrder,
    FieldOrder,
    Whitespace,
    Delimiters,
    Case,
    Macros,
    Comments,
}

fn render(entries: &[Entry], transform: Transform) -> String {
    let mut ordered: Vec<_> = entries.iter().enumerate().collect();
    if matches!(transform, Transform::EntryOrder) {
        ordered.sort_by_key(|(i, entry)| (entry.order, *i));
    }
    let mut source = String::new();
    if matches!(transform, Transform::Macros) {
        source.push_str("@string{journalname = \"Synthetic Journal\"}\n");
    }
    let inherited = "@xdata{shared,publisher={Synthetic Press}}\n";
    if !matches!(transform, Transform::EntryOrder) {
        source.push_str(inherited);
    }
    for (i, entry) in ordered {
        let mut fields = [
            ("title", entry.title.clone()),
            ("author", "García, Ada and Example, Bo".to_string()),
            ("year", entry.year.to_string()),
            ("journal", "Synthetic Journal".to_string()),
            // A warning on every record proves diagnostics are compared too.
            ("langid", "synthetic-language".to_string()),
            ("keywords", "memory, synthetic".to_string()),
            ("xdata", "shared".to_string()),
        ]
        .into_iter()
        .enumerate()
        .collect::<Vec<_>>();
        if matches!(transform, Transform::FieldOrder) {
            fields.sort_by_key(|(index, _)| (entry.fields[*index], *index));
        }
        let (kind, open, close) = match transform {
            Transform::Case => ("ArTiClE", '{', '}'),
            Transform::Delimiters => ("article", '(', ')'),
            _ => ("article", '{', '}'),
        };
        if matches!(transform, Transform::Comments) {
            source.push_str("% between entries\n@comment{ignored synthetic note}\n");
        }
        source.push_str(&format!("@{kind}{open}key{i},"));
        for (index, (name, value)) in fields {
            let name = if matches!(transform, Transform::Case) {
                name.to_uppercase()
            } else {
                name.to_string()
            };
            let value = match transform {
                Transform::Delimiters => format!("\"{value}\""),
                Transform::Macros if name == "journal" => "journalname".to_string(),
                Transform::Macros if name == "title" => {
                    let (first, rest) = value
                        .split_once(' ')
                        .unwrap_or_else(|| panic!("generated title has no space"));
                    format!("\"{first} \" # \"{rest}\"")
                }
                _ => format!("{{{value}}}"),
            };
            if matches!(transform, Transform::Whitespace) {
                source.push_str(&format!("\r\n\t  {name} \t = \r\n {value} \t ,"));
            } else {
                source.push_str(&format!("{name}={value},"));
            }
            if matches!(transform, Transform::Comments) {
                source.push_str(&format!(" % field {index}; }} ) @ ignored\n"));
            }
        }
        source.push_str(&format!("{close}\n"));
    }
    if matches!(transform, Transform::EntryOrder) {
        source.push_str(inherited);
    }
    source
}

fn canonical(mut result: LoadResult) -> LoadResult {
    for reference in &mut result.references {
        // Verbatim source text changes by definition; every normalized field
        // and the complete source/native-key identity must remain identical.
        reference.raw = None;
    }
    result.references.sort_by(|a, b| a.id.cmp(&b.id));
    for warning in &mut result.warnings {
        warning.line = None;
    }
    result
        .warnings
        .sort_by_cached_key(|w| (w.key.clone(), format!("{:?}", w.kind), w.message.clone()));
    result
}

fn equivalent(entries: &[Entry], transform: Transform) {
    let original = load_str("synthetic", &render(entries, Transform::None));
    assert_eq!(
        original.references.len(),
        entries.len(),
        "baseline must load every generated record"
    );
    assert_eq!(
        original.warnings.len(),
        entries.len(),
        "baseline must exercise warnings"
    );
    let transformed = load_str("synthetic", &render(entries, transform));
    assert_eq!(canonical(original), canonical(transformed));
}

fn config() -> ProptestConfig {
    ProptestConfig {
        // A dedicated knob keeps the normal suite bounded, even when unrelated
        // property tests set PROPTEST_CASES. Nightly exercises more libraries.
        cases: std::env::var("IBIS_METAMORPHIC_CASES").map_or(64, |value| {
            value
                .parse::<u32>()
                .ok()
                .filter(|cases| *cases > 0)
                .unwrap_or_else(|| panic!("IBIS_METAMORPHIC_CASES must be a positive integer"))
        }),
        ..ProptestConfig::default()
    }
}

#[test]
fn empty_libraries_remain_empty_under_every_transform() {
    for transform in [
        Transform::EntryOrder,
        Transform::FieldOrder,
        Transform::Whitespace,
        Transform::Delimiters,
        Transform::Case,
        Transform::Macros,
        Transform::Comments,
    ] {
        equivalent(&[], transform);
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn entry_order_preserves_identity_fields_and_warnings(entries in libraries()) {
        equivalent(&entries, Transform::EntryOrder);
    }

    #[test]
    fn field_order_preserves_identity_fields_and_warnings(entries in libraries()) {
        equivalent(&entries, Transform::FieldOrder);
    }

    #[test]
    fn whitespace_crlf_and_indentation_preserve_meaning(entries in libraries()) {
        equivalent(&entries, Transform::Whitespace);
    }

    #[test]
    fn equivalent_delimiters_preserve_meaning(entries in libraries()) {
        equivalent(&entries, Transform::Delimiters);
    }

    #[test]
    fn type_and_field_name_case_preserves_meaning(entries in libraries()) {
        equivalent(&entries, Transform::Case);
    }

    #[test]
    fn macro_expansion_and_concatenation_preserve_meaning(entries in libraries()) {
        equivalent(&entries, Transform::Macros);
    }

    #[test]
    fn comments_preserve_identity_fields_and_warnings(entries in libraries()) {
        equivalent(&entries, Transform::Comments);
    }
}
