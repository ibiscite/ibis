//! Invented inputs, checked at the boundary shared by preview and paste (#1519).
#![cfg(test)]

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
    let mapped = map_item(LibraryId::digest(b"invented"), &item).unwrap();
    Snapshot::new(vec![LibraryObject::Record(mapped.record)]).unwrap()
}

fn csl(fields: Value) -> Snapshot {
    Snapshot::new(vec![LibraryObject::Record(Record {
        added_at: None,
        id: ItemRef {
            library: LibraryId::digest(b"invented"),
            key: "work".into(),
        },
        csl: serde_json::from_value(fields).unwrap(),
        citation_keys: vec![],
        aliases: vec![],
        warnings: vec![],
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
        // Decision 0051: BibTeX identifiers are verbatim; braces belong to
        // this URL, rather than protecting prose from TeX typography.
        ("url", "https://example.org/{a--b}", "https://example.org/{a--b}"),
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
        let linked = ending.replace('&', "&#38;");
        paste(
            &result,
            &format!("Okafor, A. (2026). Imagined rivers. Invented Journal, 12–19. {ending}"),
            &format!(
                "<div class=\"csl-entry\">Okafor, A. (2026). Imagined rivers. <i>Invented Journal</i>, 12–19. <a href=\"{linked}\">{linked}</a></div>"
            ),
            &format!(
                r"Okafor, A. (2026). Imagined rivers. {{\i{{}}Invented Journal}}, 12\uc0\u8211{{}}19. {}",
                ending.replace('{', r"\{").replace('}', r"\}")
            ),
        );
    }
}

#[test]
fn only_web_addresses_and_dois_become_links() {
    // The processor would link a bare address to doi.org; it stays text.
    let formatter = Formatter::new().unwrap();
    let snapshot = bib(
        "@article{work,title={Imagined rivers},author={Okafor, Ada},year={2026},journal={Invented Journal},url={www.example.org/rivers}}",
    );
    let result = format(&formatter, &snapshot, CitationForm::BibliographyOnly);
    assert_eq!(
        result.html.as_deref(),
        Some(
            "<div class=\"csl-entry\">Okafor, A. (2026). Imagined rivers. <i>Invented Journal</i>. www.example.org/rivers</div>"
        )
    );
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
fn mapped_zotero_matches_every_existing_formatting_golden_without_a_socket() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/formatted/zotero/expected.json")).unwrap();
    let objects = fixture["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let item = serde_json::from_value(item.clone()).unwrap();
            let mapped = map_item(LibraryId::digest(b"invented"), &item).unwrap();
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
            strict: false,
            output_locale: None,
            keep_order: false,
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

fn same_paste(actual: &CitationResult, expected: &CitationResult) {
    assert_eq!(actual.text, expected.text);
    assert_eq!(actual.html, expected.html);
    assert_eq!(actual.rtf, expected.rtf);
    assert_eq!(actual.display, expected.display);
    assert!(actual.copy_content(actual.outcomes.len()).is_some());
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

#[test]
fn decoded_line_breaks_are_inline_in_every_paste_flavour() {
    let formatter = Formatter::new().unwrap();
    let source = csl(json!({"type":"article-journal","title":"Imagined&#10;rivers",
        "container-title":"Invented Journal",
        "issued":{"date-parts":[[2026]]}}));
    paste(
        &format(&formatter, &source, CitationForm::Normal),
        "(“Imagined Rivers,” 2026)",
        "(“Imagined Rivers,” 2026)",
        r"(\uc0\u8220{}Imagined Rivers,\uc0\u8221{} 2026)",
    );
}

#[test]
fn exhausting_private_code_points_refuses_and_the_next_request_recovers() {
    let formatter = Formatter::new().unwrap();
    let title: String = (0xe100..=0xf8ff)
        .chain(0xf0000..=0xffffd)
        .chain(0x100000..=0x10fffd)
        .filter_map(char::from_u32)
        .collect();
    let source = csl(json!({"type":"article-journal","title":title,"container-title":"Invented Journal"}));
    let result = format(&formatter, &source, CitationForm::Normal);
    assert_eq!(
        result.outcomes,
        [ItemOutcome::Refused {
            item: 0,
            error: CitationError::UnsupportedForm
        }]
    );
    assert_eq!(result.text, "");
    assert!(result.html.is_none() && result.rtf.is_none());
    assert!(result.copy_content(1).is_none());
    let source = csl(
        json!({"type":"article-journal","title":"Imagined &lt;i&gt;rivers&lt;/i&gt;","author":[{"family":"Okafor","given":"Ada"}],"container-title":"Invented Journal","issued":{"date-parts":[[2026]]}}),
    );
    paste(
        &format(&formatter, &source, CitationForm::BibliographyOnly),
        "Okafor, A. (2026). Imagined <i>rivers</i>. Invented Journal.",
        "<div class=\"csl-entry\">Okafor, A. (2026). Imagined &#60;i&#62;rivers&#60;/i&#62;. <i>Invented Journal</i>.</div>",
        r"Okafor, A. (2026). Imagined <i>rivers</i>. {\i{}Invented Journal}.",
    );
}

#[test]
fn raw_date_priority_controls_both_rendered_and_sorted_years() {
    let formatter = Formatter::new().unwrap();
    for raw in ["\"2022b\"", "“2022b”"] {
        let source = bib(
            "@article{a,title={Invented earlier},author={Okafor, Ada},journal={Invented Journal},year={2019}}\n\
            @article{b,title={Invented middle},author={Okafor, Ada},journal={Invented Journal},year={2020a}}\n\
            @article{c,title={Invented later},author={Okafor, Ada},journal={Invented Journal},year={2021}}",
        );
        let source = Snapshot::new(
            source
                .objects()
                .iter()
                .map(|object| {
                    let LibraryObject::Record(mut record) = object.object.clone() else {
                        unreachable!()
                    };
                    if record.id.key == "record:b" {
                        record.csl.insert(
                            "issued".into(),
                            json!({"raw":raw,"literal":"2020a","circa":true,"season":"Spring","date-parts":[]}),
                        );
                    }
                    LibraryObject::Record(record)
                })
                .collect(),
        )
        .unwrap();
        paste(
            &format(&formatter, &source, CitationForm::Normal),
            "(Okafor, 2019, 2021, 2022b)",
            "(Okafor, 2019, 2021, 2022b)",
            "(Okafor, 2019, 2021, 2022b)",
        );
        paste(
            &format(&formatter, &source, CitationForm::BibliographyOnly),
            "Okafor, A. (2019). Invented earlier. Invented Journal.\nOkafor, A. (2021). Invented later. Invented Journal.\nOkafor, A. (2022b). Invented middle. Invented Journal.",
            "<div class=\"csl-entry\">Okafor, A. (2019). Invented earlier. <i>Invented Journal</i>.</div>\n<div class=\"csl-entry\">Okafor, A. (2021). Invented later. <i>Invented Journal</i>.</div>\n<div class=\"csl-entry\">Okafor, A. (2022b). Invented middle. <i>Invented Journal</i>.</div>",
            "Okafor, A. (2019). Invented earlier. {\\i{}Invented Journal}.\\par\nOkafor, A. (2021). Invented later. {\\i{}Invented Journal}.\\par\nOkafor, A. (2022b). Invented middle. {\\i{}Invented Journal}.",
        );
    }
}
