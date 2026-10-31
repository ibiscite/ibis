#![allow(clippy::unwrap_used)]
use ibis_core::{contract::*, identifier::Identifier, search::SnapshotIndex, syntax};
use proptest::prelude::*;
use std::sync::Arc;

fn index() -> SnapshotIndex {
    index_with_keys(&["a", "b", "日本🌙", "any key", "doi"])
}

fn index_with_keys(keys: &[&str]) -> SnapshotIndex {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let LibraryObject::Record(base) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    let records = keys
        .iter()
        .copied()
        .enumerate()
        .map(|(n, key)| {
            let mut record = base.clone();
            record.id.key = key.into();
            record.citation_keys[0].value = key.into();
            record.aliases = if n == 0 { vec!["old-a".into()] } else { vec![] };
            record.identifiers = vec![Identifier::Doi(if key == "doi" {
                "10.1000/x-@a".into()
            } else {
                format!("10.1000/synthetic{n}")
            })];
            record.csl.remove("DOI");
            LibraryObject::Record(record)
        })
        .collect();
    SnapshotIndex::new(Arc::new(Snapshot::new(records).unwrap()))
}

#[test]
fn raw_lists_resolve_marker_keys_before_citation_syntax() {
    let index = index_with_keys(&["a", "b", "c", "[a]", "@a"]);
    for (query, expected) in [
        ("[a], b, c", vec!["[a]", "b", "c"]),
        ("@a, b", vec!["@a", "b"]),
        ("@a; b", vec!["@a", "b"]),
        ("@a\nb", vec!["@a", "b"]),
        ("[A], B, c", vec!["[a]", "b", "c"]),
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            expected,
            "{query}"
        );
        assert!(list.missing.is_empty(), "{query}");
    }
    let list = index
        .pasted_list("[a], @a, b PMID:999", None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(
        list.hits
            .iter()
            .map(|hit| hit.item.object.id().key.as_str())
            .collect::<Vec<_>>(),
        ["[a]", "@a", "b"]
    );
    assert_eq!(list.missing, ["PMID:999"]);
    // A bracket without a citation is still a literal token. It cannot vanish
    // and turn an incomplete raw-key list into a successful partial list.
    for query in ["[absent], b", "[absent], b, c"] {
        assert!(index.pasted_list(query, None, &|| false).unwrap().is_none(), "{query}");
    }
}

#[test]
fn json_metadata_is_not_read_as_citation_syntax() {
    let index = index();
    for field in ["citation-key", "citationKey"] {
        let query = serde_json::json!([{field: "a", "title": "}][#b]", "note": "[@absent] #cite(<b>)"}]).to_string();
        let list = index.pasted_list(&query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["a"],
            "{query}"
        );
        assert!(list.missing.is_empty(), "{query}");
        let followed = format!("{query} @b");
        let list = index.pasted_list(&followed, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
    for query in [r#"@a [{"title": "}][#b]",}] @日本🌙"#, r#"@a ["}][#b]",] @日本🌙"#] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["a", "日本🌙"],
            "{query}"
        );
        assert!(list.missing.is_empty(), "{query}");
    }
}

#[test]
fn citations_at_punctuation_boundaries_keep_hits_and_missing_keys() {
    let index = index();
    for (query, expected, missing) in [
        ("([@a]); @b", vec!["a", "b"], vec![]),
        ("([@absent]); @b", vec!["b"], vec!["absent"]),
        ("See ([@a]); @b.", vec!["a", "b"], vec![]),
        ("See (text)[@a]; @b.", vec!["a", "b"], vec![]),
        ("See (#cite(<a>)); @b", vec!["a", "b"], vec![]),
        ("See:{cite:p}`a`; @b", vec!["a", "b"], vec![]),
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            expected,
            "{query}"
        );
        assert_eq!(list.missing, missing, "{query}");
    }
}

#[test]
fn citation_syntaxes_ignore_locators_and_preserve_paste_order() {
    let index = index();
    for query in [
        "[@b; -@a, p. 4; @b]",
        "\\cite{b,a,b}",
        "\\parencite[see][pp. 4–8]{b,a,b}",
        "#cite(<b>, <a>, supplement: [p. 4])",
        "@b @a",
        "[@b; @a",
        "[[see @b]; @a]",
        "\\cite{b,a",
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        let keys: Vec<_> = list.hits.iter().map(|hit| hit.item.object.id().key.as_str()).collect();
        assert_eq!(keys, ["b", "a"], "{query}");
        assert!(list.missing.is_empty(), "{query}");
        assert!(
            list.hits
                .iter()
                .all(|hit| hit.score == 3000 && !hit.typo && !hit.partial)
        );
    }
    let list = index
        .pasted_list("@{any key} @日本🌙", None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.hits.len(), 2);
    assert_eq!(list.hits[0].item.object.id().key, "any key");
}

#[test]
fn plain_keys_and_identifiers_require_every_entry_to_match() {
    let index = index();
    for query in [
        "b\na",
        "b,a",
        "b; a",
        "b\ta",
        "B,OLD-A",
        "10.1000/synthetic1 https://doi.org/10.1000/synthetic0",
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"],
            "{query}"
        );
    }
    for query in ["", "a", "a b", "a, missing", "sleep, memory", "a@example.invalid"] {
        assert!(index.pasted_list(query, None, &|| false).unwrap().is_none(), "{query}");
    }
    let list = index
        .pasted_list("@missing @a @missing", None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.missing, ["missing"]);
    assert_eq!(list.hits.len(), 1);
    // Explicit citation syntax denotes keys, never a guessed DOI.
    let list = index
        .pasted_list("@{10.1000/synthetic0}", None, &|| false)
        .unwrap()
        .unwrap();
    assert!(list.hits.is_empty());
    assert_eq!(list.missing, ["10.1000/synthetic0"]);
}

#[test]
fn lists_keep_scope_cancellation_and_work_limits() {
    let index = index();
    let list = index.pasted_list("@a @b", Some(&[]), &|| false).unwrap().unwrap();
    assert!(list.hits.is_empty());
    assert_eq!(list.missing, ["a", "b"]);
    assert!(index.pasted_list("@a @b", None, &|| true).is_none());
    // Overflow must never turn into a silently truncated list.
    assert!(
        index
            .pasted_list(&"@a ".repeat(1001), None, &|| false)
            .unwrap()
            .is_none()
    );
    assert!(
        index
            .pasted_list(&format!("@{{{}}}", "z".repeat(65_537)), None, &|| false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn long_lists_of_prefixed_identifiers_fit_the_shared_work_allowance() {
    let index = index();
    for token in [
        "PMID: 999; ",
        "ISBN: 978 0 306 40615 7; ",
        "doi: 10.1000/absent; ",
        "arXiv: 2101.00001; ",
    ] {
        let query = token.repeat(1000);
        let list = index
            .pasted_list(&query, None, &|| false)
            .unwrap()
            .unwrap_or_else(|| panic!("not a list: {token}"));
        assert!(list.hits.is_empty());
        assert_eq!(list.missing.len(), 1, "{token}");
    }
}

#[test]
fn hostile_syntax_and_duplicate_aliases_remain_bounded() {
    let index = index();
    let list = index.pasted_list("a,OLD-A,a", None, &|| false).unwrap().unwrap();
    assert_eq!(list.hits.len(), 1);
    let list = index
        .pasted_list(&"@a ".repeat(1000), None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.hits.len(), 1);
    for query in [
        "\\cite[".repeat(8000),
        "👩🏽‍💻 @a \\cite{日本🌙".into(),
        "\\cite".repeat(10_000),
    ] {
        let _ = index.pasted_list(&query, None, &|| false).unwrap();
    }
    let checks = std::cell::Cell::new(0);
    assert!(
        index
            .pasted_list("@a @b", None, &|| {
                checks.set(checks.get() + 1);
                checks.get() > 1
            })
            .is_none()
    );
}

#[test]
fn an_at_sign_inside_a_doi_is_not_a_citation_marker() {
    let index = index();
    let list = index
        .pasted_list("10.1000/x-@a 10.1000/synthetic1", None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(
        list.hits
            .iter()
            .map(|hit| hit.item.object.id().key.as_str())
            .collect::<Vec<_>>(),
        ["doi", "b"]
    );
}

#[test]
fn multiline_arguments_and_every_biblatex_multicite_group_are_kept() {
    let index = index();
    for (query, expected) in [
        ("\\cite{a,\nb}", ["a", "b"]),
        ("\\cite{a,\tb}", ["a", "b"]),
        ("\\autocites{a}{b}", ["a", "b"]),
        ("Prose\\autocites{a}{b}", ["a", "b"]),
        ("\\parencites[see][p. 4]{b}\n[p. 8]{a}", ["b", "a"]),
        ("\\textcites[see {[}literal{]}][p. 4]{b}[p. 8]{a}", ["b", "a"]),
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            expected,
            "{query}"
        );
        assert!(list.missing.is_empty(), "{query}");
    }
    let list = index
        .pasted_list("\\autocites{a}[p. 4]{absent}{b}", None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.missing, ["absent"]);
    assert_eq!(list.hits.len(), 2);
}

#[test]
fn typst_literal_labels_decode_strings_and_keep_missing_keys() {
    let index = index();
    for query in [
        r#"@a #cite(label("日本🌙"))"#,
        r#"@a #cite(label("\u{65e5}\u{672c}\u{1f319}"))"#,
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["a", "日本🌙"]
        );
        assert!(list.missing.is_empty());
    }
    let list = index
        .pasted_list(r#"#cite(label("quote\"slash\\absent")) @a"#, None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(list.missing, ["quote\"slash\\absent"]);
    assert_eq!(list.hits.len(), 1);
}

#[test]
fn complete_identifiers_win_over_internal_citation_markers() {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let LibraryObject::Record(base) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    let records = ["a", "b", "punctuation"]
        .into_iter()
        .map(|key| {
            let mut record = base.clone();
            record.id.key = key.into();
            record.citation_keys[0].value = key.into();
            record.csl.remove("DOI");
            record.identifiers = if key == "punctuation" {
                vec![Identifier::Doi("10.1000/x;@b".into())]
            } else {
                vec![]
            };
            LibraryObject::Record(record)
        })
        .collect();
    let index = SnapshotIndex::new(Arc::new(Snapshot::new(records).unwrap()));
    for query in [
        "https://doi.org/10.1000/x;@b",
        "https://doi.org/10.1000/x%3B%40b",
        "doi:10.1000/x;@b",
        "doi: 10.1000/x;@b",
        "doi 10.1000/x;@b",
        "DOI:\t10.1000/x;@b",
        "10.1000/x;@b",
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["punctuation"],
            "{query}"
        );
        assert!(list.missing.is_empty());
    }
    let list = index
        .pasted_list("doi: 10.1000/x;@b @a", None, &|| false)
        .unwrap()
        .unwrap();
    assert_eq!(
        list.hits
            .iter()
            .map(|hit| hit.item.object.id().key.as_str())
            .collect::<Vec<_>>(),
        ["punctuation", "a"]
    );
    assert!(list.missing.is_empty());
    for token in ["https://doi.org/10.1000/absent;@b", "doi: 10.1000/absent;@b"] {
        let list = index
            .pasted_list(&format!("@a {token}"), None, &|| false)
            .unwrap()
            .unwrap();
        assert_eq!(list.hits.len(), 1, "{token}");
        assert_eq!(list.missing, [token]);
    }
}

#[test]
fn pubmed_isbn_and_arxiv_identifiers_resolve_alongside_explicit_keys() {
    let snapshot = index().snapshot().clone();
    let objects = snapshot
        .objects()
        .iter()
        .map(|item| {
            let mut object = item.object.clone();
            if let LibraryObject::Record(record) = &mut object {
                record.identifiers = match record.id.key.as_str() {
                    "a" => vec![Identifier::Pmid("12345678".into())],
                    "b" => vec![Identifier::Isbn("9780306406157".into())],
                    "日本🌙" => vec![Identifier::Arxiv("2101.00001".into())],
                    _ => vec![],
                };
            }
            object
        })
        .collect();
    let index = SnapshotIndex::new(Arc::new(Snapshot::new(objects).unwrap()));
    for (query, expected) in [
        (
            "PMID: 12345678; @b; https://arxiv.org/abs/2101.00001v3",
            ["a", "b", "日本🌙"],
        ),
        ("@a ISBN-13: 978 0 306 40615 7 arXiv:2101.00001v2", ["a", "b", "日本🌙"]),
        ("978-0-306-40615-7 PMID:12345678 arXiv:2101.00001", ["b", "a", "日本🌙"]),
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            expected,
            "{query}"
        );
        assert!(list.missing.is_empty(), "{query}");
    }
}

#[test]
fn mixed_keys_and_identifiers_keep_order_and_report_every_missing_item() {
    let index = index();
    let list = index
        .pasted_list(
            "@a; 10.1000/synthetic1; @日本🌙; https://doi.org/10.1000/synthetic0",
            None,
            &|| false,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        list.hits
            .iter()
            .map(|hit| hit.item.object.id().key.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "日本🌙"]
    );
    assert!(list.missing.is_empty());
    let list = index
        .pasted_list(
            "PMID: 999; @b; doi: 10.1000/absent; @absent; pmid:0999; 10.1000/absent",
            None,
            &|| false,
        )
        .unwrap()
        .unwrap();
    assert_eq!(list.hits.len(), 1);
    assert_eq!(list.missing, ["PMID: 999", "doi: 10.1000/absent", "absent"]);
    let list = index
        .pasted_list("PMID:999 10.1000/absent", None, &|| false)
        .unwrap()
        .unwrap();
    assert!(list.hits.is_empty());
    assert_eq!(list.missing, ["PMID:999", "10.1000/absent"]);
}

fn round_trip(caps: &syntax::SyntaxCapabilities, keys: &[String], form: CitationForm, noted: bool, title: &str) {
    let case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let LibraryObject::Record(base) = case.library.objects()[0].object.clone() else {
        panic!("record")
    };
    let mut request = case.request.clone();
    request.output = OutputSpec::Key {
        syntax: caps.syntax,
        command: caps.command.map(str::to_owned),
    };
    request.keep_order = true;
    request.form = form;
    let records: Vec<_> = keys
        .iter()
        .map(|key| {
            let mut record = base.clone();
            record.id.key = key.clone();
            record.citation_keys[0].value = key.clone();
            record.csl.insert("title".into(), title.into());
            LibraryObject::Record(record)
        })
        .collect();
    let snapshot = Arc::new(Snapshot::new(records).unwrap());
    // Each format has its documented key restrictions. Only keys the writer
    // accepts are round-trippable; every format must still render a real list.
    let items: Vec<_> = keys
        .iter()
        .map(|key| CitationItem {
            item: ItemRef {
                library: base.id.library.clone(),
                key: key.clone(),
            },
            expected_cite: None,
            prefix: if noted {
                vec![TextRun {
                    text: "see".into(),
                    ..TextRun::default()
                }]
            } else {
                vec![]
            },
            suffix: if noted {
                vec![TextRun {
                    text: "note".into(),
                    ..TextRun::default()
                }]
            } else {
                vec![]
            },
            locator: noted.then(|| Locator {
                term: LocatorTerm::Page,
                value: "4–8".into(),
            }),
            ..case.request.items[0].clone()
        })
        .filter(|item| {
            request.items = vec![item.clone()];
            syntax::cite(&snapshot, &request).unwrap().copy_content(1).is_some()
        })
        .collect();
    assert!(items.len() >= 2, "{caps:?}");
    let expected: Vec<_> = items.iter().map(|item| item.item.key.clone()).collect();
    request.items = items;
    let written = syntax::cite(&snapshot, &request).unwrap();
    assert!(
        written.copy_content(request.items.len()).is_some(),
        "{caps:?}: {:?}",
        written.outcomes
    );
    let index = SnapshotIndex::new(snapshot);
    let read = index
        .pasted_list(&written.text, None, &|| false)
        .unwrap()
        .unwrap_or_else(|| panic!("not a list: {caps:?}: {}", written.text));
    assert!(
        read.missing.is_empty(),
        "{caps:?}: {}: {:?}",
        written.text,
        read.missing
    );
    assert_eq!(
        read.hits
            .iter()
            .map(|hit| hit.item.object.id().key.clone())
            .collect::<Vec<_>>(),
        expected,
        "{caps:?}: {}",
        written.text
    );
    let followed = format!("{} PMID:999", written.text);
    let read = index
        .pasted_list(&followed, None, &|| false)
        .unwrap()
        .unwrap_or_else(|| panic!("not a list: {caps:?}: {followed}"));
    assert_eq!(read.missing, ["PMID:999"], "{caps:?}: {followed}");
    assert_eq!(
        read.hits
            .iter()
            .map(|hit| hit.item.object.id().key.clone())
            .collect::<Vec<_>>(),
        expected,
        "{caps:?}: {followed}"
    );
}

#[test]
fn every_writer_syntax_round_trips_unicode_and_punctuation_keys() {
    let keys = [
        "b",
        "a",
        "日本🌙",
        "a:b",
        "quote\"key",
        "slash\\key",
        "semi;key",
        "x@y",
        "x[bracket]",
        "x<angle>",
        "x&entity",
        "x`tick",
        "x#hash",
        "x'quote",
        "x%mark",
        "tail.",
        "backslash\\",
        "[a]",
        "@a",
    ];
    for caps in &syntax::CAPABILITIES {
        round_trip(caps, &keys.map(str::to_owned), CitationForm::Normal, false, "}][#b]");
    }
}

#[test]
fn writer_keys_with_multimarkdown_markers_round_trip() {
    // CI shrank a Pandoc failure to k][#0: the marker is literal key text
    // inside @{...}, not a MultiMarkdown note/key boundary.
    for key in ["k][#0", "k][#b]0", "k][#b][]0", "k][#b][#a]0"] {
        for caps in &syntax::CAPABILITIES {
            round_trip(
                caps,
                &["first".into(), "second".into(), key.into()],
                CitationForm::Normal,
                false,
                "",
            );
        }
    }
}

#[test]
fn writer_multikey_forms_and_notes_round_trip_without_becoming_keys() {
    for caps in &syntax::CAPABILITIES {
        for &form in caps.forms.iter().filter(|form| !caps.single_item_forms.contains(form)) {
            round_trip(
                caps,
                &["b".into(), "a".into(), "日本🌙".into()],
                form,
                true,
                "Synthetic title",
            );
        }
    }
}

fn mmd_suffix_round_trip(suffixes: [&str; 2]) {
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json")).unwrap();
    let snapshot = index().snapshot().clone();
    case.request.output = OutputSpec::Key {
        syntax: KeySyntax::MultiMarkdown,
        command: None,
    };
    case.request.items = ["a", "b"]
        .into_iter()
        .zip(suffixes)
        .map(|(key, suffix)| CitationItem {
            item: ItemRef {
                key: key.into(),
                ..case.request.items[0].item.clone()
            },
            expected_cite: None,
            prefix: vec![],
            suffix: if !suffix.is_empty() {
                vec![TextRun {
                    text: suffix.into(),
                    ..TextRun::default()
                }]
            } else {
                vec![]
            },
            locator: None,
            ..case.request.items[0].clone()
        })
        .collect();
    let written = syntax::cite(&snapshot, &case.request).unwrap();
    assert!(written.copy_content(2).is_some(), "{:?}", written.outcomes);
    let expected: String = ["a", "b"]
        .into_iter()
        .zip(suffixes)
        .map(|(key, suffix)| {
            if suffix.is_empty() {
                format!("[#{key}][]")
            } else {
                format!("[{suffix}][#{key}]")
            }
        })
        .collect();
    assert_eq!(written.text, expected);
    let index = SnapshotIndex::new(snapshot);
    for (query, missing) in [
        (written.text.clone(), vec![]),
        (format!("{} @absent", written.text), vec!["absent"]),
    ] {
        let read = index.pasted_list(&query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            read.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"],
            "{query}"
        );
        assert_eq!(read.missing, missing, "{query}");
    }
}

#[test]
fn multimarkdown_recognized_keys_can_end_without_a_closing_bracket() {
    let index = index();
    for (query, expected) in [
        ("[#a][][#b", vec!["a", "b"]),
        ("[#b", vec!["b"]),
        ("[#a][][{section][#b]", vec!["a", "b"]),
        ("[#a][][{section][#b", vec!["a", "b"]),
    ] {
        let list = index.pasted_list(query, None, &|| false).unwrap().unwrap();
        assert_eq!(
            list.hits
                .iter()
                .map(|hit| hit.item.object.id().key.as_str())
                .collect::<Vec<_>>(),
            expected,
            "{query}"
        );
        assert!(list.missing.is_empty(), "{query}");
    }
}

#[test]
fn multimarkdown_suffix_only_braces_round_trip_before_json_protection() {
    for suffix in [
        "{section",
        "}section",
        "{section}",
        "section{",
        "section}",
        "@{section",
        "@{section}",
        "@{section} @{tail",
        "@{section,tail",
        "@{section tail",
    ] {
        mmd_suffix_round_trip(["", suffix]);
    }
}

#[test]
fn multimarkdown_adjacent_notes_with_embedded_at_braces_round_trip() {
    // These notes form [x@{y][#a][}][#b]. The @{ has no Pandoc
    // key-start boundary, so it must not swallow the first reference.
    for head in ["x", "é", "0", "x-", ":", "."] {
        mmd_suffix_round_trip([&format!("{head}@{{y"), "}"]);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn multimarkdown_suffixes_with_braces_round_trip(
        head in prop::sample::select(vec!["{", "}"]),
        pieces in prop::collection::vec(prop::sample::select(vec!["{", "}", "section", "日本🌙", "é", " ", ";", ",", "@absent", ":"]), 0..16)
    ) {
        mmd_suffix_round_trip(["", format!("{head}{}", pieces.concat()).trim_end()]);
    }

    #[test]
    fn writer_lists_round_trip_generated_keys(
        pieces in prop::collection::vec((prop::sample::select(vec!["k", "[", "@"]), prop::collection::vec(prop::sample::select(vec!["日本🌙", "é", ";", "[", "]", "@", ":", ".", "/", "<", ">", "\\", "\"", "'", "%", "&", "(", ")", "#", "`", "$", "-", "!", "?", "~", "^"]), 0..7)), 1..5),
        title in prop::collection::vec(prop::sample::select(vec!["}][#b]", "[@absent]", "#cite(<absent>)", "\\cite{absent}", "日本🌙", "[", "]", "{", "}", "\"", "\\", " "]), 0..8),
        noted in any::<bool>(), narrative in any::<bool>()
    ) {
        let mut keys = vec!["first".into(), "second".into()];
        keys.extend(pieces.into_iter().enumerate().map(|(i, (head, parts))| format!("{head}{}{i}", parts.concat())));
        for caps in &syntax::CAPABILITIES {
            let form = if narrative && caps.forms.contains(&CitationForm::Narrative) && !caps.single_item_forms.contains(&CitationForm::Narrative) { CitationForm::Narrative } else { CitationForm::Normal };
            round_trip(caps, &keys, form, noted, &title.concat());
        }
    }

    #[test]
    fn damaged_mixed_syntax_stays_bounded_and_never_returns_partial_matches(
        pieces in prop::collection::vec(prop::sample::select(vec!["\\cite[", "#cite(label(\"", "\\u{", "[@", "[#", "<cite data-cite=\"", "{\"citationKey\"", "日本🌙", ";", " ", "]", "}", "\\", "\"", "@a", "10.1000/x;@b"]), 0..80)
    ) {
        let index = index();
        if let Some(list) = index.pasted_list(&pieces.concat(), None, &|| false).unwrap() {
            prop_assert!(list.hits.iter().all(|hit| !hit.partial && !hit.typo));
            let distinct: std::collections::BTreeSet<_> = list.hits.iter().map(|hit| hit.item.object.id()).collect();
            prop_assert_eq!(distinct.len(), list.hits.len());
        }
    }
}
