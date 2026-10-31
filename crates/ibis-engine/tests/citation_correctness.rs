//! Invented inputs, checked at the boundary shared by preview and paste (#1519).
#![allow(clippy::unwrap_used)]

use ibis_core::contract::*;
use ibis_csl::Formatter;
use ibis_engine::{Cancellation, Engine, request::KeyCitationRequest};
use ibis_sources::zotero::{ZoteroItem, map_item};
use serde_json::{Value, json};
use std::sync::Arc;

fn bib(text: &str) -> Snapshot {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    std::fs::write(&path, text).unwrap();
    ibis_sources::load_file(&path).unwrap().snapshot
}

fn zotero(fields: Value) -> Snapshot {
    let item: ZoteroItem = serde_json::from_value(fields).unwrap();
    let mapped = map_item(LibraryId::zotero(b"invented"), &item).unwrap();
    Snapshot::new(vec![LibraryObject::Record(mapped.record)]).unwrap()
}

fn csl(fields: Value) -> Snapshot {
    Snapshot::new(vec![LibraryObject::Record(Record {
        id: ItemRef {
            library: LibraryId::file(b"invented"),
            key: "work".into(),
        },
        csl: serde_json::from_value(fields).unwrap(),
        citation_keys: vec![],
        aliases: vec![],
        identifiers: vec![],
    })])
    .unwrap()
}

fn request(snapshot: &Snapshot, style: &str, form: CitationForm) -> CitationRequest {
    serde_json::from_value(json!({
        "items": snapshot.objects().iter().filter_map(|object| match &object.object {
            LibraryObject::Record(record) => Some(json!({"item":record.id})),
            _ => None,
        }).collect::<Vec<_>>(),
        "output":{"kind":"csl","style":style}, "form":form,
    }))
    .unwrap()
}

fn format(formatter: &Formatter, snapshot: &Snapshot, form: CitationForm) -> CitationResult {
    formatter.format(snapshot, &request(snapshot, "apa", form)).unwrap()
}

fn paste(result: &CitationResult, text: &str, html: &str, rtf_body: &str) {
    assert_eq!(result.text, text);
    assert_eq!(result.html.as_deref(), Some(html));
    assert_eq!(
        result.rtf.as_deref(),
        Some(format!("{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0 Times New Roman;}}}}\\f0\n{rtf_body}\n}}").as_str())
    );
    assert!(result.copy_content(result.outcomes.len()).is_some());
}

#[test]
fn escaped_bibtex_and_zotero_titles_paste_characters_once() {
    let formatter = Formatter::new().unwrap();
    for snapshot in [
        bib(r"@article{work,title={Salt \& pepper in imagined rivers},year={2026},journal={Invented Journal}}"),
        zotero(json!({"key":"SYN00001","itemType":"journalArticle",
            "title":"Salt & pepper in imagined rivers","date":"2026","publicationTitle":"Invented Journal"})),
    ] {
        paste(
            &format(&formatter, &snapshot, CitationForm::Normal),
            "(“Salt & Pepper in Imagined Rivers,” 2026)",
            "(“Salt &#38; Pepper in Imagined Rivers,” 2026)",
            r"(\uc0\u8220{}Salt & Pepper in Imagined Rivers,\uc0\u8221{} 2026)",
        );
    }
}

#[test]
fn bibtex_url_and_doi_dashes_remain_verbatim_but_pages_use_en_dashes() {
    let formatter = Formatter::new().unwrap();
    for (field, value, ending) in [
        ("url", "https://example.org/a--b", "https://example.org/a--b"),
        ("url", r"https\://example.org/a--b", "https://example.org/a--b"),
        ("url", "https://example.org/{a--b}", "https://example.org/a--b"),
        (
            "doi",
            "10.1000/imagined--river",
            "https://doi.org/10.1000/imagined--river",
        ),
        (
            "doi",
            r"10.1000/imagined\:river--two",
            "https://doi.org/10.1000/imagined:river--two",
        ),
        (
            "doi",
            r"10.1000/imagined\_river--two",
            "https://doi.org/10.1000/imagined_river--two",
        ),
        (
            "doi",
            r"10.1000/imagined\&river--two",
            "https://doi.org/10.1000/imagined&river--two",
        ),
    ] {
        let snapshot = bib(&format!(
            "@article{{work,title={{Imagined rivers}},author={{Okafor, Ada}},year={{2026}},journal={{Invented Journal}},pages={{12--19}},{field}={{{value}}}}}"
        ));
        let result = format(&formatter, &snapshot, CitationForm::BibliographyOnly);
        paste(
            &result,
            &format!("Okafor, A. (2026). Imagined rivers. Invented Journal, 12–19. {ending}"),
            &format!(
                "<div class=\"csl-entry\">Okafor, A. (2026). Imagined rivers. <i>Invented Journal</i>, 12–19. {}</div>",
                ending.replace('&', "&#38;")
            ),
            &format!(r"Okafor, A. (2026). Imagined rivers. {{\i{{}}Invented Journal}}, 12\uc0\u8211{{}}19. {ending}"),
        );
    }
}

fn dates_paste(dates: &[&str]) {
    let formatter = Formatter::new().unwrap();
    for date in dates {
        for snapshot in [
            bib(&format!(
                "@article{{work,title={{Imagined rivers}},author={{Okafor, Ada}},year={{{date}}}}}"
            )),
            bib(&format!(
                "@article{{work,title={{Imagined rivers}},author={{Okafor, Ada}},date={{{date}}}}}"
            )),
            zotero(
                json!({"key":"SYN00001","itemType":"journalArticle","title":"Imagined rivers",
                "creators":[{"creatorType":"author","firstName":"Ada","lastName":"Okafor"}],"date":date}),
            ),
        ] {
            let expected = format!("(Okafor, {date})");
            paste(
                &format(&formatter, &snapshot, CitationForm::Normal),
                &expected,
                &expected,
                &expected,
            );
        }
    }
}

#[test]
fn literal_dates_are_preserved() {
    dates_paste(&[
        "in press",
        "forthcoming",
        "forthcoming (2026)",
        "in press, 2026",
        "2020 (forthcoming)",
    ]);
}

#[test]
fn source_year_letters_are_preserved_without_a_doi() {
    dates_paste(&["2020a", "2020b", "2020 a"]);
}

#[test]
fn extra_and_note_date_overrides_preserve_literal_dates_and_year_letters() {
    let formatter = Formatter::new().unwrap();
    for date in ["2020a", "2020 a", "forthcoming (2026)", "in press"] {
        for source in [
            bib(&format!(
                "@article{{work,title={{Imagined rivers}},author={{Okafor, Ada}},note={{issued: {date}}}}}"
            )),
            zotero(
                json!({"key":"SYN00001","itemType":"journalArticle","title":"Imagined rivers",
                "creators":[{"creatorType":"author","firstName":"Ada","lastName":"Okafor"}],"extra":format!("issued: {date}")}),
            ),
        ] {
            let expected = format!("(Okafor, {date})");
            paste(
                &format(&formatter, &source, CitationForm::Normal),
                &expected,
                &expected,
                &expected,
            );
        }
    }
}

#[test]
fn quoted_literal_dates_keep_the_text_inside_the_quotes() {
    let formatter = Formatter::new().unwrap();
    for date in ["in press", "forthcoming", "2020a"] {
        let source = bib(&format!(
            "@article{{work,title={{Imagined rivers}},author={{Okafor, Ada}},note={{issued: \"{date}\"}}}}"
        ));
        let expected = format!("(Okafor, {date})");
        paste(
            &format(&formatter, &source, CitationForm::Normal),
            &expected,
            &expected,
            &expected,
        );
        for field in ["date", "extra"] {
            let value = if field == "extra" {
                format!("issued: \"{date}\"")
            } else {
                format!("\"{date}\"")
            };
            let source = zotero(
                json!({"key":"SYN00001","itemType":"journalArticle","title":"Imagined rivers",
                "creators":[{"creatorType":"author","firstName":"Ada","lastName":"Okafor"}],(field):value}),
            );
            let expected = format!("(Okafor, {date})");
            paste(
                &format(&formatter, &source, CitationForm::Normal),
                &expected,
                &expected,
                &expected,
            );
        }
    }
}

#[test]
fn nonbreaking_space_does_not_hide_a_literal_publication_status() {
    let formatter = Formatter::new().unwrap();
    let source = zotero(
        json!({"key":"SYN00001","itemType":"journalArticle","title":"Imagined rivers",
        "creators":[{"creatorType":"author","firstName":"Ada","lastName":"Okafor"}],"date":"in\u{a0}press (2026)"}),
    );
    paste(
        &format(&formatter, &source, CitationForm::Normal),
        "(Okafor, in\u{a0}press (2026))",
        "(Okafor, in\u{a0}press (2026))",
        r"(Okafor, in\uc0\u160{}press (2026))",
    );
}

#[test]
fn mapped_zotero_matches_every_existing_formatting_golden_without_a_socket() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/formatted/zotero/expected.json")).unwrap();
    let objects = fixture["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let item = serde_json::from_value(item.clone()).unwrap();
            let mapped = map_item(LibraryId::zotero(b"invented"), &item).unwrap();
            assert!(mapped.warnings.is_empty());
            LibraryObject::Record(mapped.record)
        })
        .collect();
    let source = Snapshot::new(objects).unwrap();
    let formatter = Formatter::new().unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases {
        let mut request = request(
            &source,
            case["style"].as_str().unwrap(),
            serde_json::from_value(case["form"].clone()).unwrap(),
        );
        request.output_locale = Some("en-US".into());
        let result = formatter.format(&source, &request).unwrap();
        assert_eq!(result.text, case["text"]);
        assert_eq!(result.html.as_deref(), case["html"].as_str());
        assert_eq!(result.rtf.as_deref(), case["rtf"].as_str());
        assert!(result.copy_content(4).is_some());
    }
}

#[test]
fn bibtex_literal_dates_containing_years_are_not_truncated() {
    let formatter = Formatter::new().unwrap();
    for field in ["year", "date"] {
        for date in ["forthcoming (2026)", "in press, 2026", "2020 a", "2020 (forthcoming)"] {
            let source = bib(&format!(
                "@article{{work,title={{Imagined rivers}},author={{Okafor, Ada}},{field}={{{date}}}}}"
            ));
            let expected = format!("(Okafor, {date})");
            paste(
                &format(&formatter, &source, CitationForm::Normal),
                &expected,
                &expected,
                &expected,
            );
        }
    }
}

#[test]
fn decoding_entities_cannot_complete_a_new_markup_instruction() {
    let formatter = Formatter::new().unwrap();
    for (title, expected) in [
        ("Imagined <i&#62;rivers</i>", "Imagined <i>rivers</i>"),
        ("Imagined <&#105;>rivers</i>", "Imagined <i>rivers</i>"),
        (
            "Imagined <span class=&quot;nocase&quot;>rivers</span>",
            "Imagined <span class=\"nocase\">rivers</span>",
        ),
    ] {
        let source = csl(
            json!({"type":"article-journal","title":title,"author":[{"family":"Okafor"}],"container-title":"Invented Journal"}),
        );
        let result = format(&formatter, &source, CitationForm::BibliographyOnly);
        paste(
            &result,
            &format!("Okafor. (n.d.). {expected}. Invented Journal."),
            &format!(
                "<div class=\"csl-entry\">Okafor. (n.d.). {}. <i>Invented Journal</i>.</div>",
                expected.replace('<', "&#60;").replace('>', "&#62;")
            ),
            &format!("Okafor. (n.d.). {expected}. {{\\i{{}}Invented Journal}}."),
        );
        assert!(
            result
                .display
                .nodes
                .iter()
                .flat_map(|node| &node.content)
                .filter(|run| run.text.contains("rivers"))
                .all(|run| !run.italic)
        );
        assert!(result.copy_content(1).is_some());
    }
}

#[test]
fn duplicate_bibtex_keys_refuse_instead_of_pasting_the_first_entry() {
    let snapshot = bib(
        "@article{work,title={First imagined river},year={2026}}\n@article{work,title={Second imagined river},year={2026}}",
    );
    let engine = Engine::from_snapshot(Arc::new(snapshot));
    let result = engine.cite_keys(
        &KeyCitationRequest {
            keys: vec!["work".into()],
            output: OutputSpec::Csl { style: "apa".into() },
            form: CitationForm::Normal,
            output_locale: None,
        },
        &Cancellation::default(),
    );
    assert_eq!(result.unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn duplicate_inheritance_parents_cannot_silently_supply_either_entries_fields() {
    let formatter = Formatter::new().unwrap();
    for parent in ["proceedings", "xdata"] {
        let source = bib(&format!(
            "@{parent}{{parent,title={{First invented parent}},year={{2020}},publisher={{First Press}}}}\n\
             @{parent}{{parent,title={{Second invented parent}},year={{2021}},publisher={{Second Press}}}}\n\
             @inproceedings{{child,title={{Imagined child}},{reference}={{parent}}}}",
            reference = if parent == "xdata" { "xdata" } else { "crossref" }
        ));
        assert_eq!(source.objects().len(), 1);
        let expected = csl(json!({"type":"paper-conference","title":"Imagined child"}));
        for form in [CitationForm::Normal, CitationForm::BibliographyOnly] {
            same_paste(&format(&formatter, &source, form), &format(&formatter, &expected, form));
        }
    }
}

#[test]
fn bidi_controls_cannot_reorder_any_paste_flavour() {
    let formatter = Formatter::new().unwrap();
    for snapshot in [
        bib("@article{work,title={Imagined \u{202e}rivers},year={2026},journal={Invented Journal}}"),
        zotero(
            json!({"key":"SYN00001","itemType":"journalArticle","title":"Imagined \u{202e}rivers","date":"2026","publicationTitle":"Invented Journal"}),
        ),
        csl(
            json!({"type":"article-journal","title":"Imagined \u{202e}rivers","container-title":"Invented Journal","issued":{"date-parts":[[2026]]}}),
        ),
    ] {
        paste(
            &format(&formatter, &snapshot, CitationForm::Normal),
            "(“Imagined Rivers,” 2026)",
            "(“Imagined Rivers,” 2026)",
            r"(\uc0\u8220{}Imagined Rivers,\uc0\u8221{} 2026)",
        );
    }
}

#[test]
fn removing_controls_cannot_create_a_new_markup_instruction() {
    let formatter = Formatter::new().unwrap();
    for title in [
        "Imagined <\u{202e}i>rivers</i>",
        "Imagined <span class=\"no\u{202e}case\">rivers</span>",
    ] {
        let source = csl(json!({"type":"article-journal","title":title,"container-title":"Invented Journal"}));
        assert_eq!(
            formatter.format(&source, &request(&source, "apa", CitationForm::Normal)),
            Err(ibis_csl::FormatError::InvalidRequest)
        );
    }
}

fn same_paste(actual: &CitationResult, expected: &CitationResult) {
    assert_eq!(actual.text, expected.text);
    assert_eq!(actual.html, expected.html);
    assert_eq!(actual.rtf, expected.rtf);
    assert_eq!(actual.display, expected.display);
    assert!(actual.copy_content(actual.outcomes.len()).is_some());
}

#[test]
fn escapes_in_titles_names_containers_and_publishers_match_literal_csl() {
    let formatter = Formatter::new().unwrap();
    let source = bib(
        r"@book{work,title={Imagined \& rivers},author={{A \& B}},publisher={C \&amp; D},address={E \& F},year={2026}}",
    );
    let expected = csl(
        json!({"type":"book","title":"Imagined & rivers","author":[{"literal":"A & B"}],
        "publisher":"C &amp; D","publisher-place":"E & F","issued":{"date-parts":[[2026]]}}),
    );
    for (style, _) in ibis_csl::STYLES {
        for form in [CitationForm::Normal, CitationForm::BibliographyOnly] {
            same_paste(
                &formatter.format(&source, &request(&source, style, form)).unwrap(),
                &formatter.format(&expected, &request(&expected, style, form)).unwrap(),
            );
        }
    }
    paste(
        &format(&formatter, &source, CitationForm::BibliographyOnly),
        "A & B. (2026). Imagined & rivers. C &amp; D.",
        "<div class=\"csl-entry\">A &#38; B. (2026). <i>Imagined &#38; rivers</i>. C &#38;amp; D.</div>",
        r"A & B. (2026). {\i{}Imagined & rivers}. C &amp; D.",
    );
}

#[test]
fn decoded_entities_stay_text_and_cannot_create_markup_or_collide_with_placeholders() {
    let formatter = Formatter::new().unwrap();
    for snapshot in [
        bib(
            r"@article{work,title={Imagined \textless{}i\textgreater{}rivers\textless{}/i\textgreater{} \&amp; \textit{blue}},author={Okafor, Ada},journal={Sea \& shore},year={2026}}",
        ),
        zotero(json!({"key":"SYN00001","itemType":"journalArticle",
            "title":"Imagined &lt;i&gt;rivers&lt;/i&gt; &amp;amp; <i>blue</i>",
            "creators":[{"creatorType":"author","firstName":"Ada","lastName":"Okafor"}],
            "publicationTitle":"Sea &#38; shore","date":"2026"})),
    ] {
        paste(
            &format(&formatter, &snapshot, CitationForm::BibliographyOnly),
            "Okafor, A. (2026). Imagined <i>rivers</i> &amp; blue. Sea & Shore.",
            "<div class=\"csl-entry\">Okafor, A. (2026). Imagined &#60;i&#62;rivers&#60;/i&#62; &#38;amp; <i>blue</i>. <i>Sea &#38; Shore</i>.</div>",
            r"Okafor, A. (2026). Imagined <i>rivers</i> &amp; {\i{}blue}. {\i{}Sea & Shore}.",
        );
    }
    let source = csl(json!({"type":"book","title":"Invented &#60;i&#62; &#x1F319; &#57346;&#57347; \u{e002}\u{e003}"}));
    let result = format(&formatter, &source, CitationForm::BibliographyOnly);
    assert_eq!(
        result.text,
        "Invented <i> 🌙 \u{e002}\u{e003} \u{e002}\u{e003}. (n.d.)."
    );
    assert!(result.copy_content(1).is_some());
}

#[test]
fn bidi_controls_in_entities_affixes_and_locators_are_removed_but_rtl_text_is_kept() {
    let formatter = Formatter::new().unwrap();
    let clean = csl(
        json!({"type":"book","title":"عنوان می\u{200c}خی","author":[{"family":"Okafor"}],"issued":{"date-parts":[[2026]]}}),
    );
    let mut clean_request = request(&clean, "apa", CitationForm::Normal);
    clean_request.items[0].prefix = vec![TextRun {
        text: "see ".into(),
        ..TextRun::default()
    }];
    clean_request.items[0].suffix = vec![TextRun {
        text: ", note".into(),
        ..TextRun::default()
    }];
    clean_request.items[0].locator = Some(Locator {
        term: LocatorTerm::Page,
        value: "12".into(),
    });
    let expected = formatter.format(&clean, &clean_request).unwrap();
    assert_eq!(expected.text, "(see Okafor, 2026, p. 12, note)");
    for control in [
        '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}',
        '\u{2067}', '\u{2068}', '\u{2069}', '\u{206a}', '\u{206f}',
    ] {
        let mut dirty_request = clean_request.clone();
        dirty_request.items[0].prefix[0].text.push(control);
        dirty_request.items[0].suffix[0].text.push(control);
        dirty_request.items[0].locator.as_mut().unwrap().value.push(control);
        same_paste(&formatter.format(&clean, &dirty_request).unwrap(), &expected);
        for encoded in [control.to_string(), format!("&#{};", control as u32)] {
            let dirty = csl(
                json!({"type":"book","title":format!("عنوان{encoded} می\u{200c}خی"),"author":[{"family":"Okafor"}],"issued":{"date-parts":[[2026]]}}),
            );
            same_paste(
                &format(&formatter, &dirty, CitationForm::BibliographyOnly),
                &format(&formatter, &clean, CitationForm::BibliographyOnly),
            );
        }
    }
}

#[test]
fn explicit_year_letters_survive_clusters_and_never_generate_new_letters() {
    let formatter = Formatter::new().unwrap();
    let source = bib(
        "@article{a,title={First imagined river},author={Okafor, Ada},year={2020a}}\n@article{b,title={Second imagined river},author={Okafor, Ada},year={2020b}}",
    );
    let result = format(&formatter, &source, CitationForm::Normal);
    assert!(!result.notices.contains(&Notice::YearSuffixWithheld));
    paste(
        &result,
        "(Okafor, 2020a, 2020b)",
        "(Okafor, 2020a, 2020b)",
        "(Okafor, 2020a, 2020b)",
    );
    let source = bib(
        "@article{a,title={First imagined river},author={Okafor, Ada},year={2020}}\n@article{b,title={Second imagined river},author={Okafor, Ada},year={2020}}",
    );
    let result = format(&formatter, &source, CitationForm::Normal);
    paste(
        &result,
        "(Okafor, 2020, 2020)",
        "(Okafor, 2020, 2020)",
        "(Okafor, 2020, 2020)",
    );
    assert!(result.notices.contains(&Notice::YearSuffixWithheld));
}
