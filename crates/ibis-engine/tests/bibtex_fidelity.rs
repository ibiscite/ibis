//! Invented source-to-paste regressions for #1530.
#![cfg(test)]
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
        map_item(LibraryId::digest(b"invented"), &item).unwrap().record,
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

/// A style's bibliography layout in copied HTML and RTF, as Zotero writes it.
struct Layout {
    html: &'static str,
    rtf: &'static str,
}
const APA: Layout = Layout {
    html: "line-height: 2; margin-left: 2em; text-indent:-2em;",
    rtf: r"\li720 \fi-720 \sl480 \slmult1 \sa0 ",
};
const IEEE: Layout = Layout {
    html: "line-height: 1.35; ",
    rtf: r"\li0 \fi0 \sl240 \slmult1 \sa0 ",
};

fn paste(result: &CitationResult, layout: &Layout, text: &str, html: &str, rtf: &str) {
    assert_eq!(result.text, text);
    let html = format!("<div class=\"csl-bib-body\" style=\"{}\">{html}</div>", layout.html);
    assert_eq!(result.html.as_deref(), Some(html.as_str()));
    assert_eq!(
        result.rtf,
        Some(format!(
            "{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0 Times New Roman;}}}}\\f0 {}\n{rtf}\n}}",
            layout.rtf
        ))
    );
    assert!(result.copy_content(result.outcomes.len()).is_some());
}

/// `tail` is "" or " URL"; HTML links the URL, as Zotero does.
fn apa_article(result: &CitationResult, plain: &str, html: &str, rtf: &str, tail: &str) {
    let linked = tail
        .strip_prefix(' ')
        .map_or_else(String::new, |url| format!(" <a href=\"{url}\">{url}</a>"));
    paste(
        result,
        &APA,
        &format!("Okafor, A. (2026). {plain}. Invented Journal.{tail}"),
        &format!("<div class=\"csl-entry\">Okafor, A. (2026). {html}. <i>Invented Journal</i>.{linked}</div>"),
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
                &IEEE,
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
fn accepted_circa_flags_export_and_reimport_as_one_approximation() {
    let formatter = Formatter::new().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invented.json");
    for (circa, approximate) in [
        (json!(true), true),
        (json!(1), true),
        (json!("1"), true),
        (json!(-2), true),
        (json!(0.5), true),
        (json!("true"), true),
        (json!("0"), true),
        (json!("false"), true),
        (json!(false), false),
        (json!(0), false),
        (json!(""), false),
    ] {
        std::fs::write(&path, json!([{"id":"work", "citation-key":"work", "type":"book", "title":"Invented atlas", "issued":{"date-parts":[[2020,3]], "circa":circa}}]).to_string()).unwrap();
        let source = ibis_sources::load_file(&path).unwrap().snapshot;
        let mut request: CitationRequest = serde_json::from_value(json!({
            "items":[{"item":source.objects()[0].object.id()}],
            "output":{"kind":"key", "syntax":"bibtex"}
        }))
        .unwrap();
        let exported = ibis_core::syntax::cite(&source, &request).unwrap();
        let expected = if approximate {
            "date = {2020-03~}"
        } else {
            "date = {2020-03}"
        };
        assert!(exported.text.contains(expected), "{circa}: {}", exported.text);
        let reimported = bib(&exported.text);
        let LibraryObject::Record(record) = &reimported.objects()[0].object else {
            unreachable!()
        };
        assert_eq!(
            record.csl["issued"]
                .get("circa")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            approximate
        );
        request.output = OutputSpec::Key {
            syntax: KeySyntax::CslJson,
            command: None,
        };
        let exported: Value = serde_json::from_str(&ibis_core::syntax::cite(&source, &request).unwrap().text).unwrap();
        assert_eq!(exported[0]["issued"]["circa"], approximate, "{circa}");
        assert_eq!(
            format(&formatter, &source, "apa").text,
            format(&formatter, &reimported, "apa").text
        );
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
