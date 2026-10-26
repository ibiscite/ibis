use serde_json::json;

use super::*;
use crate::model::{AttachmentKind, PartialDate};

/// A small synthetic global schema in the shape Zotero stores.
fn schema() -> Schema {
    let global = json!({
        "version": 1,
        "meta": { "fields": { "date": { "type": "date" }, "originalDate": { "type": "date" } } },
        "csl": {
            "types": {
                "article-journal": ["journalArticle"],
                "book": ["book"],
                "song": ["audioRecording"],
                "speech": ["presentation"]
            },
            "fields": {
                "text": {
                    "citation-key": ["citationKey"],
                    "container-title": ["publicationTitle"],
                    "event-place": ["eventPlace"],
                    "ISBN": ["ISBN"],
                    "note": ["extra"],
                    "publisher": ["publisher"],
                    "publisher-place": ["place"],
                    "shortTitle": ["shortTitle"],
                    "title": ["title"],
                    "title-short": ["shortTitle"]
                },
                "date": { "accessed": "accessDate", "issued": "date", "original-date": "originalDate" }
            },
            "names": { "author": "author", "editor": "editor", "performer": "author" }
        }
    });
    let base = [
        ("audioRecording", "publisher", "label"),
        ("presentation", "date", "dateGiven"),
    ]
    .map(|(t, b, f)| (t.to_string(), b.to_string(), f.to_string()));
    let primary = [("presentation", "presenter")].map(|(t, c)| (t.to_string(), c.to_string()));
    Schema::new(global.to_string().as_bytes(), base, primary).unwrap_or_else(|| panic!("schema"))
}

fn item(key: &str, item_type: &str, fields: &[(&str, &str)]) -> Item {
    Item {
        key: key.into(),
        version: 3,
        item_type: item_type.into(),
        date_added: "2024-01-02 03:04:05".into(),
        date_modified: "2024-01-02 03:04:05".into(),
        fields: fields.iter().map(|(f, v)| (f.to_string(), v.to_string())).collect(),
        ..Item::default()
    }
}

fn person(creator_type: &str, first: &str, last: &str) -> Creator {
    Creator {
        creator_type: creator_type.into(),
        first_name: first.into(),
        last_name: last.into(),
        single_field: false,
    }
}

#[test]
fn a_stored_article_reads_like_the_local_api_sends_it() {
    let mut article = item(
        "ABCD2345",
        "journalArticle",
        &[
            ("title", "Sleep spindles predict memory"),
            ("shortTitle", "Sleep spindles"),
            ("publicationTitle", "Journal of Synthetic Studies"),
            ("date", "2021-03-14 March 14, 2021"),
            ("accessDate", "2024-01-02 23:59:00"),
            ("extra", "Citation Key: garcia2021\nOriginal Date: 2001"),
        ],
    );
    article.creators = vec![
        person("author", "Lucía", "García"),
        person("author", "Vincent", "van Gogh"),
        Creator {
            creator_type: "author".into(),
            last_name: "Synthetic Consortium".into(),
            single_field: true,
            ..Creator::default()
        },
        person("editor", "Hana", "Ito"),
    ];
    article.tags = vec![("sleep".into(), 0), ("memory".into(), 1)];
    article.attachment_type = Some("application/pdf".into());
    let schema = schema();
    let api = api_item(&schema, &article);
    assert_eq!(api["data"]["date"], "March 14, 2021", "data keeps what the user typed");
    assert_eq!(api["data"]["dateAdded"], "2024-01-02T03:04:05Z");
    assert_eq!(
        api["data"]["creators"][2],
        json!({ "creatorType": "author", "name": "Synthetic Consortium" })
    );
    assert_eq!(api["data"]["tags"][1], json!({ "tag": "memory", "type": 1 }));
    let csl = &api["csljson"];
    assert_eq!(csl["title-short"], "Sleep spindles");
    assert!(csl.get("shortTitle").is_none());
    assert_eq!(csl["note"], "citation-key: garcia2021\noriginal-date: 2001");
    assert_eq!(csl["issued"], json!({ "date-parts": [[2021, 3, 14]] }));
    assert_eq!(csl["accessed"], json!({ "date-parts": [[2024, 1, 2]] }));
    assert_eq!(csl["author"][1]["non-dropping-particle"], "van");

    let result = load(Library::Group(7), &schema, &[article]);
    assert_eq!(result.warnings, []);
    let r = &result.references[0];
    assert_eq!(r.id.source, "zotero:group:7");
    assert_eq!(r.citekey.as_deref(), Some("garcia2021"));
    assert_eq!(r.item_type, "article-journal");
    assert_eq!(r.authors.len(), 3);
    assert_eq!(r.authors[2].literal.as_deref(), Some("Synthetic Consortium"));
    assert_eq!(r.editors[0].family.as_deref(), Some("Ito"));
    assert_eq!(r.container_title.as_deref(), Some("Journal of Synthetic Studies"));
    assert_eq!(r.keywords, ["sleep", "memory"]);
    assert_eq!(r.attachment, Some(AttachmentKind::Pdf));
    let issued = r.issued.as_ref().map(|i| i.start);
    assert_eq!(
        issued,
        Some(PartialDate {
            year: 2021,
            month: Some(3),
            day: Some(14)
        })
    );
}

#[test]
fn the_native_citation_key_wins_and_type_specific_fields_fill_base_variables() {
    let mut song = item(
        "SONG0001",
        "audioRecording",
        &[
            ("citationKey", "own2020"),
            ("label", "Example Records"),
            ("place", "Lagos"),
        ],
    );
    song.creators = vec![person("performer", "Ada", "Example")];
    let mut talk = item(
        "TALK0001",
        "presentation",
        &[("place", "Kyoto"), ("dateGiven", "2019-00-00 2019")],
    );
    talk.creators = vec![person("presenter", "Ben", "Sample")];
    let schema = schema();
    let song_csl = &api_item(&schema, &song)["csljson"];
    assert_eq!(song_csl["publisher"], "Example Records");
    assert_eq!(
        song_csl["event-place"], "Lagos",
        "a recording's place is its event's place"
    );
    assert!(song_csl.get("publisher-place").is_none());
    let talk_csl = &api_item(&schema, &talk)["csljson"];
    assert_eq!(talk_csl["issued"], json!({ "date-parts": [[2019]] }));
    assert_eq!(
        api_item(&schema, &talk)["data"]["dateGiven"],
        "2019",
        "fields mapped to a date are dates too"
    );
    assert_eq!(
        talk_csl["author"][0]["family"], "Sample",
        "the primary creator is the author"
    );

    let result = load(Library::User, &schema, &[song, talk]);
    assert_eq!(result.references[0].citekey.as_deref(), Some("own2020"));
}

#[test]
fn dates_without_a_year_quoted_dates_and_odd_values_stay_text() {
    let schema = schema();
    let csl_date_of =
        |date: &str| api_item(&schema, &item("K", "book", &[("date", date)]))["csljson"]["issued"].clone();
    assert_eq!(
        csl_date_of("0000-00-00 Spring term"),
        json!({ "literal": "Spring term" })
    );
    assert_eq!(
        csl_date_of("2001-00-00 \"circa 2001\""),
        json!({ "literal": "circa 2001" })
    );
    assert_eq!(csl_date_of("2001-05-00 May 2001"), json!({ "date-parts": [[2001, 5]] }));
    assert_eq!(csl_date_of("not two-part"), json!({ "literal": "not two-part" }));
    assert_eq!(csl_date_of("2001-05-00"), json!({ "literal": "2001-05-00" }));
}

#[test]
fn text_values_follow_zoteros_export_rules() {
    let schema = schema();
    let csl = &api_item(
        &schema,
        &item(
            "K",
            "book",
            &[
                ("ISBN", "978-0-306-40615-7 0-306-40615-2"),
                ("title", "\"Quoted title\""),
                ("publisher", "\"Half \" quoted\""),
                (
                    "extra",
                    "Publication Title: X\nDOI: 10.1/x\nnot a field\nArchive Location:\n shelf 3\nBoxed:",
                ),
            ],
        ),
    )["csljson"];
    assert_eq!(csl["ISBN"], "978-0-306-40615-7");
    assert_eq!(csl["title"], "Quoted title");
    assert_eq!(csl["publisher"], "\"Half \" quoted\"");
    assert_eq!(
        csl["note"],
        "container-title: X\nDOI: 10.1/x\nnot a field\narchive_location:\n shelf 3\nBoxed:"
    );
    assert_eq!(first_isbn("0-306-40615-2x"), None);
    assert_eq!(first_isbn("123456789X;"), Some("123456789X"));
    assert_eq!(strip_quotes("\""), "\"");
}

#[test]
fn an_item_type_without_a_csl_type_is_reported_not_dropped_silently() {
    let result = load(
        Library::User,
        &schema(),
        &[item("ODD00001", "customThing", &[("title", "x")])],
    );
    assert!(result.references.is_empty());
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].key.as_deref(), Some("ODD00001"));
}

#[test]
fn a_schema_without_csl_mappings_is_refused() {
    assert!(Schema::new(b"{}", [], []).is_none());
    assert!(Schema::new(b"not json", [], []).is_none());
    assert!(Schema::new(br#"{"csl":{"types":[],"fields":{},"names":{}}}"#, [], []).is_none());
}

proptest::proptest! {
    #[test]
    fn any_stored_text_converts_without_a_crash(text in "\\PC{0,80}", other in "[A-Za-z :\\-\n\"“”0-9,!']{0,60}") {
        let schema = schema();
        for value in [&text, &other] {
            let mut stored = item("K", "journalArticle", &[("title", value), ("extra", value), ("date", value), ("ISBN", value)]);
            stored.creators = vec![person("author", value, value), person("editor", &other, &text)];
            let _ = load(Library::User, &schema, &[stored]);
            let _ = extra_to_csl(&schema, value);
            let _ = csl_date(value, true);
        }
    }
}

#[test]
fn checked_streaming_keeps_duplicate_key_diagnostics_and_rejects_partial_results() {
    let schema = schema();
    let items = [
        item("FIRST001", "book", &[("citationKey", "shared")]),
        item("SECOND01", "book", &[("citationKey", "shared")]),
    ];
    let expected = load(Library::User, &schema, &items);
    let checked = load_checked(Library::User, &schema, &items, |_| Ok::<_, ()>(())).unwrap_or_else(|_| panic!("load"));
    assert_eq!(checked, expected);
    assert_eq!(checked.warnings.len(), 1);
    let mut calls = 0;
    let stopped = load_checked(Library::User, &schema, &items, |_| {
        calls += 1;
        if calls == 2 { Err("stopped") } else { Ok(()) }
    });
    assert_eq!(stopped, Err("stopped"));
}
