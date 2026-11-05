//! One reading of a record for every front end (#1675). The picker's rows, the
//! command line's search and show, MCP's results and the search index all read
//! a record's title, people, year and container here, so the same record shows
//! the same year and the same names everywhere. Wording ("No date",
//! "[untitled]") stays with each front end; what the record says does not.
use crate::model::{Name, strip_markup};
use serde_json::Value;

/// The longest date text a summary keeps; longer text ends with "…".
pub const DATE_TEXT_LIMIT: usize = 20;
/// The longest container a summary keeps; longer text ends with "…".
pub const CONTAINER_LIMIT: usize = 120;

/// What every front end shows of a record. Missing parts are `None` or empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordSummary {
    /// The title without markup.
    pub title: Option<String>,
    /// The authors in their order, each named once.
    pub authors: Vec<Author>,
    /// The year as digits when the date has one, otherwise the source's own
    /// date text ("circa spring 1890s"), cut to [`DATE_TEXT_LIMIT`].
    pub year: Option<String>,
    /// The journal, book or other container, else the publisher.
    pub container: Option<String>,
    /// The CSL item type, such as `article-journal`.
    pub item_type: Option<String>,
}

/// One author, as a reading line and as a short name for compact rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    /// In reading order: "Ana García", "国立研究所", or "et al." for a list
    /// the source truncated.
    pub name: String,
    /// The family name, or the whole name when it has no family part.
    pub short: String,
}

impl RecordSummary {
    /// The summary of a record's CSL fields, read through `field`.
    pub fn of<'a>(field: impl Fn(&str) -> Option<&'a Value>) -> Self {
        let text = |name: &str| {
            field(name)
                .and_then(Value::as_str)
                .map(strip_markup)
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
        };
        let authors = field("author")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(author)
            .collect();
        Self {
            title: text("title"),
            authors,
            year: date_text(field("issued")),
            container: text("container-title")
                .or_else(|| text("publisher"))
                .map(|text| capped(&text, CONTAINER_LIMIT)),
            item_type: text("type"),
        }
    }

    /// The authors' reading names, joined with commas.
    pub fn author_names(&self) -> String {
        self.authors
            .iter()
            .map(|author| author.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn author(value: &Value) -> Option<Author> {
    author_named(Name::from_csl(value))
}

fn author_named(name: Name) -> Option<Author> {
    let display = strip_markup(&name.display()).trim().to_owned();
    if display.is_empty() {
        return None;
    }
    let short = if name.is_others() {
        None
    } else {
        name.literal.as_deref().or(name.family.as_deref()).map(strip_markup)
    }
    .map(|short| short.trim().to_owned())
    .filter(|short| !short.is_empty())
    .unwrap_or_else(|| display.clone());
    Some(Author { name: display, short })
}

/// The year a date gives as a number: CSL `date-parts` (a number or digits),
/// else an ISO-like `raw` or `literal` date ("2016", "2016-05-01"). Text such as
/// "circa spring 1890s" has none. Search reads years through this.
pub fn issued_year(issued: Option<&Value>) -> Option<i32> {
    let issued = issued?;
    if let Some(year) = issued.pointer("/date-parts/0/0") {
        return match year {
            Value::Number(number) => number.as_i64().and_then(|year| i32::try_from(year).ok()),
            Value::String(text) => digits_year(text.trim()),
            _ => None,
        };
    }
    ["raw", "literal"]
        .into_iter()
        .filter_map(|part| issued.get(part).and_then(Value::as_str))
        .find_map(iso_year)
}

/// What a summary says for a date: its year, else its own text.
fn date_text(issued: Option<&Value>) -> Option<String> {
    if let Some(year) = issued_year(issued) {
        return Some(year.to_string());
    }
    let issued = issued?;
    ["literal", "raw"]
        .into_iter()
        .filter_map(|part| issued.get(part).and_then(Value::as_str))
        .map(|text| strip_markup(text).split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|text| !text.is_empty())
        .map(|text| capped(&text, DATE_TEXT_LIMIT))
}

fn digits_year(text: &str) -> Option<i32> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    (!digits.is_empty() && digits.len() <= 4 && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// "2016", "2016-05" or "2016-05-01" (also with `/`): the year.
fn iso_year(text: &str) -> Option<i32> {
    let numeric = |part: &str, lengths: std::ops::RangeInclusive<usize>| {
        lengths.contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_digit())
    };
    let mut parts = text.trim().split(['-', '/']);
    let year = parts.next()?;
    let rest: Vec<_> = parts.collect();
    (numeric(year, 4..=4) && rest.len() <= 2 && rest.iter().all(|part| numeric(part, 1..=2)))
        .then(|| year.parse().ok())
        .flatten()
}

/// `text` cut to at most `limit` characters, ending with an ellipsis.
pub fn capped(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let kept: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

impl crate::contract::Record {
    /// What every front end shows of this record.
    pub fn summary(&self) -> RecordSummary {
        // Compact names are read as they are held, building no JSON.
        let compact = self.csl.names("author");
        let mut summary = RecordSummary::of(|field| {
            if field == "author" && compact.is_some() {
                None
            } else {
                self.csl.get(field)
            }
        });
        if let Some(names) = compact {
            summary.authors = names
                .iter()
                .map(|name| name.to_name())
                .filter_map(author_named)
                .collect();
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn summary(csl: Value) -> RecordSummary {
        RecordSummary::of(|field| csl.get(field))
    }

    #[test]
    fn a_year_reads_the_same_however_the_source_wrote_it() {
        for (issued, year) in [
            (json!({"date-parts": [[2016]]}), Some("2016")),
            (json!({"date-parts": [["2016"]]}), Some("2016")),
            (json!({"date-parts": [[2016, 5, 1]], "circa": true}), Some("2016")),
            (json!({"raw": "2016-05-01"}), Some("2016")),
            (json!({"literal": "2016"}), Some("2016")),
            (json!({"raw": "circa spring 1890s"}), Some("circa spring 1890s")),
            (json!({"literal": "<i>n.d.</i>  forthcoming"}), Some("n.d. forthcoming")),
            (
                json!({"raw": "in the reign of an invented monarch"}),
                Some("in the reign of an…"),
            ),
            (json!({"date-parts": [[]]}), None),
            (json!({"raw": "   "}), None),
            (json!({}), None),
        ] {
            assert_eq!(summary(json!({"issued": issued})).year.as_deref(), year, "{issued}");
        }
        assert_eq!(summary(json!({})).year, None);
    }

    #[test]
    fn search_finds_a_year_only_where_the_date_gives_one() {
        for (issued, year) in [
            (json!({"date-parts": [["2016"]]}), Some(2016)),
            (json!({"date-parts": [[-44]]}), Some(-44)),
            (json!({"raw": "2016/05"}), Some(2016)),
            (json!({"raw": "2016-05-01"}), Some(2016)),
            (json!({"raw": "circa spring 1890s"}), None),
            (json!({"raw": "1890s"}), None),
            (json!({"raw": "20160501"}), None),
            (json!({"raw": "2016-"}), None),
            (json!({"date-parts": [["spring"]]}), None),
        ] {
            assert_eq!(issued_year(Some(&issued)), year, "{issued}");
        }
    }

    #[test]
    fn people_are_named_once_in_reading_order_with_a_short_form() {
        let found = summary(json!({"author": [
            {"family": "García", "given": "Ana"},
            {"literal": "Invented <i>Group</i>"},
            {"given": "Mononym"},
            {"family": "国立", "given": "研究所"},
            {},
            {"literal": "others"},
        ]}));
        let names: Vec<_> = found
            .authors
            .iter()
            .map(|a| (a.name.as_str(), a.short.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("Ana García", "García"),
                ("Invented Group", "Invented Group"),
                ("Mononym", "Mononym"),
                ("国立研究所", "国立"),
                ("et al.", "et al."),
            ]
        );
        assert_eq!(
            found.author_names(),
            "Ana García, Invented Group, Mononym, 国立研究所, et al."
        );
    }

    #[test]
    fn title_and_container_are_plain_text_and_the_publisher_stands_in() {
        let found =
            summary(json!({"title": " <i>Synthetic</i> tides ", "publisher": "Invented Press", "type": "book"}));
        assert_eq!(found.title.as_deref(), Some("Synthetic tides"));
        assert_eq!(found.container.as_deref(), Some("Invented Press"));
        assert_eq!(found.item_type.as_deref(), Some("book"));
        let journal =
            summary(json!({"container-title": "Journal of Tests", "publisher": "Invented Press", "title": ""}));
        assert_eq!(journal.container.as_deref(), Some("Journal of Tests"));
        assert_eq!(journal.title, None);
        let long = summary(json!({"container-title": "x".repeat(200)}));
        assert_eq!(long.container.map(|c| c.chars().count()), Some(CONTAINER_LIMIT));
    }
}
