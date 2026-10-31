//! Invented source-to-paste regressions for #1530.
#![allow(clippy::unwrap_used)]
use ibis_core::contract::*;
use ibis_csl::Formatter;
use ibis_sources::zotero::{ZoteroItem, map_item};
use serde_json::{Value, json};

fn bib(fields: &str) -> Snapshot {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invented.bib");
    std::fs::write(&path, fields).unwrap();
    ibis_sources::load_file(&path).unwrap().snapshot
}

fn article(fields: &str) -> Snapshot {
    bib(&format!(
        "@article{{work,author={{Okafor, Ada}},year={{2026}},journal={{Invented Journal}},{fields}}}"
    ))
}

fn zotero(fields: Value) -> Snapshot {
    let item: ZoteroItem = serde_json::from_value(fields).unwrap();
    Snapshot::new(vec![LibraryObject::Record(
        map_item(LibraryId::zotero(b"invented"), &item).unwrap().record,
    )])
    .unwrap()
}

fn zotero_article(title: &str, extra: Value) -> Snapshot {
    let mut item = json!({"key":"SYN00001","itemType":"journalArticle","title":title,
        "date":"2026","publicationTitle":"Invented Journal",
        "creators":[{"creatorType":"author","firstName":"Ada","lastName":"Okafor"}]});
    item.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    zotero(item)
}

fn format(formatter: &Formatter, source: &Snapshot, style: &str) -> CitationResult {
    let request: CitationRequest = serde_json::from_value(json!({
        "items":source.objects().iter().map(|o|json!({"item":o.object.id()})).collect::<Vec<_>>(),
        "output":{"kind":"csl","style":style},"form":"bibliography-only"
    }))
    .unwrap();
    formatter.format(source, &request).unwrap()
}

fn paste(result: &CitationResult, text: &str, html: &str, rtf: &str) {
    assert_eq!(result.text, text);
    assert_eq!(result.html.as_deref(), Some(html));
    assert_eq!(
        result.rtf,
        Some(format!(
            "{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0 Times New Roman;}}}}\\f0\n{rtf}\n}}"
        ))
    );
    assert!(result.copy_content(result.outcomes.len()).is_some());
}

fn apa_article(result: &CitationResult, plain: &str, html: &str, rtf: &str, tail: &str) {
    paste(
        result,
        &format!("Okafor, A. (2026). {plain}. Invented Journal.{tail}"),
        &format!("<div class=\"csl-entry\">Okafor, A. (2026). {html}. <i>Invented Journal</i>.{tail}</div>"),
        &format!("Okafor, A. (2026). {rtf}. {{\\i{{}}Invented Journal}}.{tail}"),
    );
}

#[test]
fn stored_proper_nouns_acronyms_and_protected_braces_keep_their_case() {
    let formatter = Formatter::new().unwrap();
    for title in [
        "Climate Change in Japan and Germany",
        "Climate Change in {Japan} and {Germany}",
        "Using {iPhone} Apps for {NASA} and DNA Research",
    ] {
        let plain = title.replace(['{', '}'], "");
        let source = article(&format!("title={{{title}}}"));
        let native = zotero_article(&plain, json!({}));
        for source in [&source, &native] {
            apa_article(&format(&formatter, source, "apa"), &plain, &plain, &plain, "");
            paste(
                &format(&formatter, source, "ieee"),
                &format!("A. Okafor, “{plain},” Invented Journal, 2026."),
                &format!("<div class=\"csl-entry\">A. Okafor, “{plain},” <i>Invented Journal</i>, 2026.</div>"),
                &format!("A. Okafor, \\uc0\\u8220{{}}{plain},\\uc0\\u8221{{}} {{\\i{{}}Invented Journal}}, 2026."),
            );
        }
    }
}

#[test]
fn resolver_and_label_doi_forms_print_one_resolver_prefix() {
    let formatter = Formatter::new().unwrap();
    for doi in [
        "10.1234/invented--river",
        "https://doi.org/10.1234/invented--river",
        "doi:10.1234/invented--river",
        "DOI: 10.1234/invented--river",
        "http://dx.doi.org/10.1234/invented--river",
        "HTTPS://DOI.ORG/10.1234/invented--river",
        "doi.org/10.1234/invented--river",
        "DX.DOI.ORG/10.1234/invented--river",
    ] {
        for source in [
            article(&format!("title={{Invented rivers}},doi={{{doi}}}")),
            zotero_article("Invented rivers", json!({"DOI":doi})),
        ] {
            apa_article(
                &format(&formatter, &source, "apa"),
                "Invented rivers",
                "Invented rivers",
                "Invented rivers",
                " https://doi.org/10.1234/invented--river",
            );
        }
    }
}

#[test]
fn doi_suffix_typography_is_verbatim_and_case_and_percent_bytes_are_preserved() {
    let formatter = Formatter::new().unwrap();
    for doi in [
        "10.1234/INVENTED~River%2Fnotes",
        "HTTPS://DX.DOI.ORG/10.1234/INVENTED~River%2Fnotes",
    ] {
        for source in [
            article(&format!("title={{Invented rivers}},doi={{{doi}}}")),
            zotero_article("Invented rivers", json!({"DOI":doi})),
        ] {
            // citeproc URL-encodes the literal percent character; the source
            // adapter must not decode it into a different DOI path component.
            apa_article(
                &format(&formatter, &source, "apa"),
                "Invented rivers",
                "Invented rivers",
                "Invented rivers",
                " https://doi.org/10.1234/INVENTED~River%252Fnotes",
            );
        }
    }
}

#[test]
fn tex_ties_quotes_and_escaped_symbols_print_their_meaning() {
    let formatter = Formatter::new().unwrap();
    for (tex, plain, html, rtf) in [
        ("Figure~1", "Figure\u{a0}1", "Figure\u{a0}1", r"Figure\uc0\u160{}1"),
        (
            r"Figure\nobreakspace{}1",
            "Figure\u{a0}1",
            "Figure\u{a0}1",
            r"Figure\uc0\u160{}1",
        ),
        (
            "``Rivers'' notes",
            "“Rivers” notes",
            "“Rivers” notes",
            r"\uc0\u8220{}Rivers\uc0\u8221{} notes",
        ),
        (
            "`Rivers' notes",
            "‘Rivers’ notes",
            "‘Rivers’ notes",
            r"\uc0\u8216{}Rivers\uc0\u8217{} notes",
        ),
        (r"Salt \& pepper", "Salt & pepper", "Salt &#38; pepper", "Salt & pepper"),
        ("Figure\u{a0}1", "Figure\u{a0}1", "Figure\u{a0}1", r"Figure\uc0\u160{}1"),
        (
            "Figure\u{202f}1",
            "Figure\u{202f}1",
            "Figure\u{202f}1",
            r"Figure\uc0\u8239{}1",
        ),
        (
            r"\textquotedblleft{}Rivers\textquotedblright{} notes",
            "“Rivers” notes",
            "“Rivers” notes",
            r"\uc0\u8220{}Rivers\uc0\u8221{} notes",
        ),
        (
            r"\textquoteleft{}Rivers\textquoteright{} notes",
            "‘Rivers’ notes",
            "‘Rivers’ notes",
            r"\uc0\u8216{}Rivers\uc0\u8217{} notes",
        ),
    ] {
        apa_article(
            &format(&formatter, &article(&format!("title={{{tex}}}")), "apa"),
            plain,
            html,
            rtf,
            "",
        );
        let native = html.replace("&#38;", "&");
        apa_article(
            &format(&formatter, &zotero_article(&native, json!({})), "apa"),
            plain,
            html,
            rtf,
            "",
        );
    }
}

#[test]
fn latex_quote_commands_and_literal_ascii_symbols_remain_distinct() {
    let formatter = Formatter::new().unwrap();
    for (tex, plain, rtf) in [
        (
            r"\glqq{}Rivers\grqq{} notes",
            "„Rivers“ notes",
            r"\uc0\u8222{}Rivers\uc0\u8220{} notes",
        ),
        (
            r"\glq{}Rivers\grq{} notes",
            "‚Rivers‘ notes",
            r"\uc0\u8218{}Rivers\uc0\u8216{} notes",
        ),
        (
            r"\quotedblbase{}Rivers\textquotedblleft{} notes",
            "„Rivers“ notes",
            r"\uc0\u8222{}Rivers\uc0\u8220{} notes",
        ),
        (
            r"\quotesinglbase{}Rivers\textquoteleft{} notes",
            "‚Rivers‘ notes",
            r"\uc0\u8218{}Rivers\uc0\u8216{} notes",
        ),
        (
            r"\guillemotleft{}Rivers\guillemotright{} notes",
            "«Rivers» notes",
            r"\uc0\u171{}Rivers\uc0\u187{} notes",
        ),
        (
            r"\guilsinglleft{}Rivers\guilsinglright{} notes",
            "‹Rivers› notes",
            r"\uc0\u8249{}Rivers\uc0\u8250{} notes",
        ),
        (
            r"\lq{}Rivers\rq{} notes",
            "‘Rivers’ notes",
            r"\uc0\u8216{}Rivers\uc0\u8217{} notes",
        ),
        (
            r"Literal \textasciitilde{} and \textasciigrave{} symbols",
            "Literal ~ and ` symbols",
            "Literal ~ and ` symbols",
        ),
    ] {
        apa_article(
            &format(&formatter, &article(&format!("title={{{tex}}}")), "apa"),
            plain,
            plain,
            rtf,
            "",
        );
        apa_article(
            &format(&formatter, &zotero_article(plain, json!({})), "apa"),
            plain,
            plain,
            rtf,
            "",
        );
    }
}

#[test]
fn paired_tex_quotes_leave_punctuation_placement_to_citeproc() {
    let formatter = Formatter::new().unwrap();
    for source in [article("title={``Rivers''}"), zotero_article("“Rivers”", json!({}))] {
        paste(
            &format(&formatter, &source, "apa"),
            "Okafor, A. (2026). “Rivers.” Invented Journal.",
            "<div class=\"csl-entry\">Okafor, A. (2026). “Rivers.” <i>Invented Journal</i>.</div>",
            r"Okafor, A. (2026). \uc0\u8220{}Rivers.\uc0\u8221{} {\i{}Invented Journal}.",
        );
    }
}

#[test]
fn double_backslashes_in_urls_cannot_be_decoded_twice() {
    let formatter = Formatter::new().unwrap();
    for url in [r"https://example.org/a\\_b", r"https://example.org/\textbackslash{}b"] {
        let source = article(&format!("title={{Invented rivers}},url={{{url}}}"));
        let result = format(&formatter, &source, "apa");
        apa_article(&result, "Invented rivers", "Invented rivers", "Invented rivers", "");
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.code == CitationWarningCode::SourceField)
        );
    }
}

#[test]
fn unknown_and_implicit_languages_never_lowercase_german_nouns() {
    let formatter = Formatter::new().unwrap();
    for language in [
        "",
        ",hyphenation={german}",
        ",language={Deutsch}",
        ",langid={invented-language}",
    ] {
        let source = article(&format!("title={{Die Karten der stillen Flüsse}}{language}"));
        apa_article(
            &format(&formatter, &source, "apa"),
            "Die Karten der stillen Flüsse",
            "Die Karten der stillen Flüsse",
            r"Die Karten der stillen Fl\uc0\u252{}sse",
            "",
        );
    }
}

#[test]
fn apa_capitalizes_subtitles_through_the_processor_for_every_source() {
    let formatter = Formatter::new().unwrap();
    for source in [
        article("title={Mapping rivers: a field guide}"),
        zotero_article("Mapping rivers: a field guide", json!({})),
    ] {
        apa_article(
            &format(&formatter, &source, "apa"),
            "Mapping rivers: A field guide",
            "Mapping rivers: A field guide",
            "Mapping rivers: A field guide",
            "",
        );
    }
}

#[test]
fn exported_tex_url_escapes_remain_public_urls() {
    let formatter = Formatter::new().unwrap();
    for (tex, expected) in [
        (r"https://example.org/a\_b", "https://example.org/a_b"),
        (r"https://example.org/a\%20b", "https://example.org/a%20b"),
        (r"https://example.org/a\&b", "https://example.org/a&b"),
    ] {
        let result = format(
            &formatter,
            &article(&format!("title={{Invented rivers}},url={{{tex}}}")),
            "apa",
        );
        let tail = format!(" {expected}");
        paste(
            &result,
            &format!("Okafor, A. (2026). Invented rivers. Invented Journal.{tail}"),
            &format!(
                "<div class=\"csl-entry\">Okafor, A. (2026). Invented rivers. <i>Invented Journal</i>. {}</div>",
                expected.replace('&', "&#38;")
            ),
            &format!("Okafor, A. (2026). Invented rivers. {{\\i{{}}Invented Journal}}.{tail}"),
        );
    }
}

#[test]
fn howpublished_url_and_href_supply_the_public_url() {
    let formatter = Formatter::new().unwrap();
    for command in [
        r"\url{https://example.org/invented--river}",
        r"\href{https://example.org/invented--river}{Invented archive}",
    ] {
        apa_article(
            &format(
                &formatter,
                &article(&format!("title={{Invented rivers}},howpublished={{{command}}}")),
                "apa",
            ),
            "Invented rivers",
            "Invented rivers",
            "Invented rivers",
            " https://example.org/invented--river",
        );
    }
}

#[test]
fn ambiguous_howpublished_text_warns_instead_of_guessing_a_link() {
    let formatter = Formatter::new().unwrap();
    for field in [
        r"Available at \url{https://example.org/work}",
        r"\url{https://example.org/work}\relax",
    ] {
        let result = format(
            &formatter,
            &article(&format!("title={{Invented rivers}},howpublished={{{field}}}")),
            "apa",
        );
        apa_article(&result, "Invented rivers", "Invented rivers", "Invented rivers", "");
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.code == CitationWarningCode::SourceField)
        );
    }
    let source = article(
        r"title={Invented rivers},url={https://example.org/explicit},howpublished={\url{https://example.org/fallback}}",
    );
    apa_article(
        &format(&formatter, &source, "apa"),
        "Invented rivers",
        "Invented rivers",
        "Invented rivers",
        " https://example.org/explicit",
    );
}

#[test]
fn software_and_dataset_versions_reach_formatted_entries() {
    let formatter = Formatter::new().unwrap();
    for (kind, label) in [("software", "Computer software"), ("dataset", "Data set")] {
        let source = bib(&format!(
            "@{kind}{{work,title={{Invented rivers}},author={{Okafor, Ada}},year={{2026}},version={{2.0}}}}"
        ));
        let native = zotero(
            json!({"key":"SYN00001","itemType":if kind == "software" {"computerProgram"} else {"dataset"},
            "title":"Invented rivers","date":"2026","versionNumber":"2.0",
            "creators":[{"creatorType":if kind == "software" {"programmer"} else {"author"},"firstName":"Ada","lastName":"Okafor"}]}),
        );
        for source in [&source, &native] {
            paste(
                &format(&formatter, source, "apa"),
                &format!("Okafor, A. (2026). Invented rivers (Version 2.0) [{label}]."),
                &format!(
                    "<div class=\"csl-entry\">Okafor, A. (2026). <i>Invented rivers</i> (Version 2.0) [{label}].</div>"
                ),
                &format!("Okafor, A. (2026). {{\\i{{}}Invented rivers}} (Version 2.0) [{label}]."),
            );
        }
    }
}

#[test]
fn math_does_not_change_the_case_of_adjacent_text() {
    let formatter = Formatter::new().unwrap();
    for source in [
        article("title={H$_2$O notes}"),
        zotero_article("H<sub>2</sub>O notes", json!({})),
    ] {
        apa_article(
            &format(&formatter, &source, "apa"),
            "H2O notes",
            "H<sub>2</sub>O notes",
            r"H\sub 2\nosupersub{}O notes",
            "",
        );
    }
}

#[test]
fn source_field_warnings_follow_only_the_selected_item_to_the_result() {
    let formatter = Formatter::new().unwrap();
    let source =
        article(r"title={Invented rivers},language={unreadable-language},publisher={\unknown{Invented Press}}");
    let result = format(&formatter, &source, "apa");
    let warnings = serde_json::to_value(&result).unwrap();
    assert!(
        warnings["warnings"]
            .as_array()
            .is_some_and(|warnings| warnings.contains(&json!({"item":0,"code":"source-field"})))
    );
    apa_article(&result, "Invented rivers", "Invented rivers", "Invented rivers", "");
    let objects = source
        .objects()
        .iter()
        .map(|object| object.object.clone())
        .collect::<Vec<_>>();
    let mut clean_record = match objects[0].clone() {
        LibraryObject::Record(record) => record,
        _ => unreachable!(),
    };
    clean_record.warnings.clear();
    let clean = Snapshot::new(vec![LibraryObject::Record(clean_record.clone())]).unwrap();
    let clean_result = format(&formatter, &clean, "apa");
    assert!(
        !clean_result
            .warnings
            .iter()
            .any(|warning| warning.code == CitationWarningCode::SourceField)
    );
    assert_eq!(clean_result.content_hash, result.content_hash);
    clean_record.id.key = "clean".into();
    let mut combined = objects;
    combined.push(LibraryObject::Record(clean_record.clone()));
    let combined = Snapshot::new(combined).unwrap();
    let request: CitationRequest = serde_json::from_value(
        json!({"items":[{"item":clean_record.id}],"output":{"kind":"csl","style":"apa"},"form":"bibliography-only"}),
    )
    .unwrap();
    assert!(
        !formatter
            .format(&combined, &request)
            .unwrap()
            .warnings
            .iter()
            .any(|warning| warning.code == CitationWarningCode::SourceField)
    );
    let repeated: CitationRequest = serde_json::from_value(json!({"items":[{"item":source.objects()[0].object.id()},{"item":source.objects()[0].object.id()}],"output":{"kind":"csl","style":"apa"}})).unwrap();
    assert_eq!(
        formatter.format(&source, &repeated).unwrap().warnings,
        vec![
            CitationWarning {
                item: 0,
                code: CitationWarningCode::SourceField
            },
            CitationWarning {
                item: 1,
                code: CitationWarningCode::SourceField
            }
        ]
    );
}
