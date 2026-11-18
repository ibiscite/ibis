//! EndNote's fields as CSL, for the first slice: journal articles, books
//! and book sections, with their people, title, year, container, publisher,
//! volume, issue, pages, edition and DOI. EndNote's XML tags are generic
//! (`secondary-title` is a journal, a series or a book by type); the roles
//! below were checked against EndNote 21.3's own field labels for each type.
use ibis_core::contract::adapter::{AdapterSkip, AdapterSkipCause};
use ibis_sources::csl_parts::{date_parts, doi, person};
use serde_json::{Map, Value, json};

use crate::xml::XmlRecord;

/// The record as CSL without its `id`, or why it was left out.
pub(crate) fn csl(record: &XmlRecord) -> Result<Map<String, Value>, AdapterSkip> {
    let title = record.one("titles/title");
    let skip = |cause, field: &str| AdapterSkip {
        cause,
        entry: title.map(str::to_owned),
        field: Some(field.into()),
    };
    // A field EndNote writes once; several values mean the record is not one EndNote made.
    let one = |name: &str| -> Result<Option<&str>, AdapterSkip> {
        match record.fields.get(name).map(Vec::as_slice) {
            None | Some([]) => Ok(None),
            Some([value]) => Ok(Some(value.as_str())),
            Some(_) => Err(skip(AdapterSkipCause::Conflicting, name)),
        }
    };
    let kind = match one("ref-type")? {
        Some("17") => "article-journal",
        Some("6") => "book",
        Some("5") => "chapter",
        _ => return Err(skip(AdapterSkipCause::UnsupportedType, "ref-type")),
    };
    let title = one("titles/title")?.ok_or_else(|| skip(AdapterSkipCause::MissingField, "title"))?;
    let mut csl = Map::new();
    csl.insert("type".into(), json!(kind));
    csl.insert("title".into(), json!(title));
    let people: &[(&str, &str)] = match kind {
        // Book: Series Editor, then Editor.
        "book" => &[
            ("contributors/authors/author", "author"),
            ("contributors/secondary-authors/author", "collection-editor"),
            ("contributors/tertiary-authors/author", "editor"),
        ],
        _ => &[
            ("contributors/authors/author", "author"),
            ("contributors/secondary-authors/author", "editor"),
        ],
    };
    for (source, role) in people {
        let names: Vec<Value> = record
            .fields
            .get(*source)
            .into_iter()
            .flatten()
            .filter_map(|name| endnote_person(name))
            .collect();
        if !names.is_empty() {
            csl.insert((*role).into(), Value::Array(names));
        }
    }
    let fields: &[(&str, &str)] = &[
        // Book: Number of Pages; otherwise Pages.
        ("pages", if kind == "book" { "number-of-pages" } else { "page" }),
        ("volume", "volume"),
        ("edition", "edition"),
        // Journal Article: Issue.
        ("number", if kind == "article-journal" { "issue" } else { "number" }),
        ("publisher", "publisher"),
        ("pub-location", "publisher-place"),
        // Journal, Series Title or Book Title. `periodical` is EndNote's
        // journal list entry, which may be spelled differently.
        (
            "titles/secondary-title",
            if kind == "book" {
                "collection-title"
            } else {
                "container-title"
            },
        ),
    ];
    for (source, target) in fields {
        if let Some(value) = one(source)? {
            csl.insert((*target).into(), json!(value));
        }
    }
    if !csl.contains_key("container-title")
        && kind == "article-journal"
        && let Some(journal) = one("periodical/full-title")?
    {
        csl.insert("container-title".into(), json!(journal));
    }
    // Ibis checks every DOI again; one that is not a DOI is its to drop and report.
    if let Some(written) = one("electronic-resource-num")? {
        csl.insert("DOI".into(), json!(doi(written).unwrap_or_else(|| written.to_owned())));
    }
    // A year that is not four digits stays as written.
    if let Some(year) = one("dates/year")? {
        csl.insert(
            "issued".into(),
            date_parts(year, None, None).unwrap_or_else(|| json!({"literal": year})),
        );
    }
    Ok(csl)
}

/// EndNote marks a whole organisation name with a trailing comma, and a
/// comma inside one by doubling it.
fn endnote_person(name: &str) -> Option<Value> {
    let name = name.trim();
    if let Some((first, rest)) = name.split_once(",,") {
        Some(json!({"literal": format!("{first},{rest}")}))
    } else if let Some(organisation) = name.strip_suffix(',') {
        let organisation = organisation.trim();
        (!organisation.is_empty()).then(|| json!({"literal": organisation}))
    } else {
        person(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapped(xml: &str) -> Vec<Result<Map<String, Value>, AdapterSkip>> {
        crate::xml::records(xml)
            .unwrap_or_else(|_| panic!("reads"))
            .iter()
            .map(csl)
            .collect()
    }

    #[test]
    fn the_invented_library_maps_as_endnote_labels_its_fields() {
        let mapped = mapped(include_str!("../../../fixtures/endnote/invented.xml"));
        let expected: Vec<Value> =
            serde_json::from_str(include_str!("../../../fixtures/endnote/expected.json")).unwrap_or_default();
        let kept: Vec<Value> = mapped
            .iter()
            .filter_map(|r| r.clone().ok())
            .map(Value::Object)
            .collect();
        assert_eq!(kept, expected);
        let skipped: Vec<_> = mapped.into_iter().filter_map(Result::err).collect();
        assert_eq!(
            skipped,
            [AdapterSkip {
                cause: AdapterSkipCause::UnsupportedType,
                entry: Some("An invented talk".into()),
                field: Some("ref-type".into()),
            }]
        );
    }

    #[test]
    fn organisations_repeats_and_bad_values_are_never_guessed() {
        let xml = "<xml><records><record><ref-type>6</ref-type><titles><title>Invented book</title></titles><contributors><authors><author>Invented Institute,</author><author>Invented Institute,, Research Division</author><author>Roe, Rafael</author></authors></contributors></record></records></xml>";
        let Ok(book) = &mapped(xml)[0] else { panic!("kept") };
        assert_eq!(
            book["author"],
            json!([{"literal":"Invented Institute"},{"literal":"Invented Institute, Research Division"},{"family":"Roe","given":"Rafael"}])
        );
        for (inner, field) in [
            ("<titles><title>A</title><title>B</title></titles>", "titles/title"),
            (
                "<titles><title>A</title></titles><volume>1</volume><volume>2</volume>",
                "volume",
            ),
            ("<titles></titles>", "title"),
        ] {
            let xml = format!("<xml><records><record><ref-type>17</ref-type>{inner}</record></records></xml>");
            let Err(skip) = &mapped(&xml)[0] else { panic!("{inner}") };
            assert_eq!(skip.field.as_deref(), Some(field), "{inner}");
        }
        let xml = "<xml><records><record><ref-type>17</ref-type><titles><title>A</title></titles><dates><year>26</year></dates><electronic-resource-num>https://doi.org/10.0000/invented</electronic-resource-num></record></records></xml>";
        let Ok(article) = &mapped(xml)[0] else { panic!("kept") };
        assert_eq!(article["issued"], json!({"literal": "26"}));
        assert_eq!(article["DOI"], "10.0000/invented");
        // The journal list's spelling is used only when the record has none.
        let xml = "<xml><records><record><ref-type>17</ref-type><titles><title>A</title><secondary-title>Invented J</secondary-title></titles><periodical><full-title>Invented Journal</full-title></periodical></record></records></xml>";
        let Ok(article) = &mapped(xml)[0] else { panic!("kept") };
        assert_eq!(article["container-title"], "Invented J");
    }
}
