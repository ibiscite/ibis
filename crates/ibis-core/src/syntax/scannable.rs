//! Zotero's scannable citation markers, which Zotero's RTF Scan and the ODF Scan
//! add-on turn into live Zotero citations (#1531):
//! `{ prefix | label | locator | suffix | identifier }`, one per reference,
//! joined with nothing between. They name Zotero items, not citation keys, so
//! only a reference the engine read from a Zotero library can be written, and
//! the library comes from the engine's own record of what it read, never from
//! anything the record says. The marker has no escape for `|`, `{` or `}`:
//! text that would need one is refused, and the whole citation with it.
use super::{MAX_CITATION_ITEMS, MAX_OUTPUT_BYTES, validate_annotation, validate_request};
use crate::contract::*;
use crate::summary::RecordSummary;

/// The Zotero library a reference was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoteroOrigin {
    Personal,
    Group(u64),
}

/// The locators RTF and ODF Scan read, as they write them.
const LOCATORS: [(LocatorTerm, &str); 15] = [
    (LocatorTerm::Page, "p."),
    (LocatorTerm::Chapter, "ch."),
    (LocatorTerm::Section, "sec."),
    (LocatorTerm::Volume, "vol."),
    (LocatorTerm::Figure, "fig."),
    (LocatorTerm::ArticleLocator, "art."),
    (LocatorTerm::Column, "col."),
    (LocatorTerm::Line, "l."),
    (LocatorTerm::Note, "n."),
    (LocatorTerm::Issue, "no."),
    (LocatorTerm::Opus, "op."),
    (LocatorTerm::Paragraph, "para."),
    (LocatorTerm::Part, "pt."),
    (LocatorTerm::Rule, "r."),
    (LocatorTerm::Verse, "vrs."),
];

/// Zotero's legal item types take their pinpoints in their own fields, so a
/// locator on one would be read as something else.
const LEGAL_TYPES: [&str; 6] = ["legal_case", "legislation", "bill", "hearing", "regulation", "treaty"];

/// Characters the marker reads as its own; `*` marks italic and bold.
const RESERVED: [char; 4] = ['|', '{', '}', '*'];

/// Markers for every item, or each item's refusal and no text: a marker
/// Zotero cannot read fails quietly, only when the document is refreshed.
pub fn cite(
    snapshot: &Snapshot,
    request: &CitationRequest,
    origin: impl Fn(&LibraryId) -> Option<ZoteroOrigin>,
) -> Result<CitationResult, CitationError> {
    if request.items.is_empty() {
        return Err(CitationError::EmptySelection);
    }
    if request.items.len() > MAX_CITATION_ITEMS {
        return Err(CitationError::TooManyItems);
    }
    validate_request(request)?;
    let mut outcomes = Vec::with_capacity(request.items.len());
    let mut markers = Vec::with_capacity(request.items.len());
    let mut warnings = Vec::new();
    for (index, item) in request.items.iter().enumerate() {
        match marker(snapshot, request, item, &origin) {
            Ok((text, record)) => {
                warnings.extend(record.warnings.iter().map(|code| CitationWarning {
                    item: index,
                    code: *code,
                }));
                markers.push(DisplayNode {
                    item: Some(index),
                    content: vec![DisplayRun {
                        text,
                        italic: false,
                        bold: false,
                        script: Script::Normal,
                        small_caps: false,
                        underline: false,
                    }],
                });
                outcomes.push(ItemOutcome::Rendered { item: index });
            }
            Err(error) => outcomes.push(ItemOutcome::Refused { item: index, error }),
        }
    }
    let refused = outcomes.iter().any(|outcome| matches!(outcome, ItemOutcome::Refused { .. }));
    let display = DisplayTree {
        nodes: if refused { vec![] } else { markers },
    };
    let text: String = display.nodes.iter().flat_map(|node| &node.content).map(|run| run.text.as_str()).collect();
    if text.len() > MAX_OUTPUT_BYTES {
        return Err(CitationError::OutputTooLarge);
    }
    Ok(CitationResult::new(text, None, None, display, outcomes, vec![], vec![]).with_warnings(warnings))
}

fn marker<'a>(
    snapshot: &'a Snapshot,
    request: &CitationRequest,
    item: &CitationItem,
    origin: &impl Fn(&LibraryId) -> Option<ZoteroOrigin>,
) -> Result<(String, &'a Record), CitationError> {
    let object = snapshot.get(&item.item).ok_or(CitationError::MissingItem)?;
    if item
        .expected_cite
        .as_ref()
        .is_some_and(|expected| expected != &object.fingerprints.cite)
    {
        return Err(CitationError::ChangedItem);
    }
    let LibraryObject::Record(record) = &object.object else {
        return Err(CitationError::NotCitable);
    };
    validate_annotation(snapshot, item)?;
    if item.form.unwrap_or(request.form) != CitationForm::Normal {
        return Err(CitationError::UnsupportedForm);
    }
    let identifier = identifier(origin(&record.id.library), &record.id.key)?;
    let locator = match &item.locator {
        None => String::new(),
        Some(locator) => {
            let legal = record
                .csl
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| LEGAL_TYPES.contains(&kind));
            let label = LOCATORS
                .iter()
                .find(|(term, _)| *term == locator.term)
                .map(|(_, label)| *label)
                .filter(|_| !legal)
                .ok_or(CitationError::UnsupportedForm)?;
            let value = locator.value.trim();
            if value.is_empty() || value.contains(RESERVED) || value.chars().any(char::is_control) {
                return Err(CitationError::InvalidLocator);
            }
            format!("{label} {value}")
        }
    };
    let label = label(record);
    let label = if item.suppress_author { format!("-{label}") } else { label };
    let fields = [affix(&item.prefix)?, label, locator, affix(&item.suffix)?, identifier];
    let text = fields
        .iter()
        .map(|field| if field.is_empty() { " ".to_owned() } else { format!(" {field} ") })
        .collect::<Vec<_>>()
        .join("|");
    Ok((format!("{{{text}}}"), record))
}

/// `zu:0:KEY` for the personal library, `zg:GROUP:KEY` for a group: never
/// the numeric user ID, which a document should not carry.
fn identifier(origin: Option<ZoteroOrigin>, key: &str) -> Result<String, CitationError> {
    // Zotero's item keys: eight of its 32 letters and digits.
    let valid = key.len() == 8 && key.bytes().all(|byte| b"23456789ABCDEFGHIJKLMNPQRSTUVWXYZ".contains(&byte));
    match origin {
        Some(ZoteroOrigin::Personal) if valid => Ok(format!("zu:0:{key}")),
        Some(ZoteroOrigin::Group(group)) if valid && group > 0 => Ok(format!("zg:{group}:{key}")),
        _ => Err(CitationError::NotFromZotero),
    }
}

/// What the marker shows until Zotero replaces it: the first author and the
/// year, else the title. It is display only, so text it cannot carry is dropped.
fn label(record: &Record) -> String {
    let summary = RecordSummary::of(|name| record.csl.get(name));
    let name = summary
        .authors
        .first()
        .map(|author| author.short.clone())
        .or_else(|| summary.title.map(|title| title.chars().take(60).collect()));
    let plain = |part: String| {
        let part: String = part
            .chars()
            .map(|ch| if ch.is_control() || RESERVED.contains(&ch) { ' ' } else { ch })
            .collect();
        part.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let parts: Vec<String> = name.into_iter().chain(summary.year).map(plain).filter(|part| !part.is_empty()).collect();
    // A leading `-` would ask Zotero to leave the author out.
    parts.join(", ").trim_start_matches('-').trim().to_owned()
}

/// Plain text with at most one italic and one bold span, written `*so*` and
/// `**so**`; anything more, or a character the marker reserves, is refused.
fn affix(runs: &RichText) -> Result<String, CitationError> {
    let mut spans: Vec<(bool, bool, String)> = Vec::new();
    for run in runs.iter().filter(|run| !run.text.is_empty()) {
        if run.script != Script::Normal
            || run.italic && run.bold
            || run.text.contains(RESERVED)
            || run.text.chars().any(char::is_control)
        {
            return Err(CitationError::InvalidRichText);
        }
        match spans.last_mut() {
            Some((italic, bold, text)) if *italic == run.italic && *bold == run.bold => text.push_str(&run.text),
            _ => spans.push((run.italic, run.bold, run.text.clone())),
        }
    }
    let count = |pick: fn(&(bool, bool, String)) -> bool| spans.iter().filter(|span| pick(span)).count();
    if count(|span| span.0) > 1 || count(|span| span.1) > 1 {
        return Err(CitationError::InvalidRichText);
    }
    let text: String = spans
        .iter()
        .map(|(italic, bold, text)| match (italic, bold) {
            // The marks go round the words, so a space stays outside them.
            (true, _) | (_, true) => {
                let marks = if *italic { "*" } else { "**" };
                let words = text.trim();
                let (before, after) = (&text[..text.len() - text.trim_start().len()], &text[text.trim_end().len()..]);
                if words.is_empty() { text.clone() } else { format!("{before}{marks}{words}{marks}{after}") }
            }
            _ => text.clone(),
        })
        .collect();
    Ok(text.trim().to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    const PERSONAL: &[u8] = b"zotero-personal";
    const GROUP: &[u8] = b"zotero-group";

    fn snapshot() -> Snapshot {
        let record = |library: &[u8], key: &str, csl: serde_json::Value| {
            LibraryObject::Record(Record {
                added_at: None,
                id: ItemRef {
                    library: LibraryId::digest(library),
                    key: key.into(),
                },
                csl: serde_json::from_value(csl).unwrap(),
                citation_keys: vec![],
                aliases: vec![],
                warnings: vec![],
                identifiers: vec![],
            })
        };
        let book = json!({"type":"book","title":"Invented atlas","author":[{"family":"Example","given":"Avery"}],"issued":{"date-parts":[[2026]]}});
        Snapshot::new(vec![
            record(PERSONAL, "ABCD2345", book.clone()),
            record(GROUP, "WXYZ6789", json!({"type":"book","title":"Untitled | {braced} *atlas*","issued":{"date-parts":[[2025]]}})),
            record(b"bibtex-file", "example2026", book),
            record(PERSONAL, "LEGAL234", json!({"type":"legal_case","title":"Invented v. Example"})),
            record(PERSONAL, "lowercase", json!({"type":"book","title":"Invented"})),
        ])
        .unwrap()
    }

    fn origin(library: &LibraryId) -> Option<ZoteroOrigin> {
        if *library == LibraryId::digest(PERSONAL) {
            Some(ZoteroOrigin::Personal)
        } else if *library == LibraryId::digest(GROUP) {
            Some(ZoteroOrigin::Group(4321))
        } else {
            None
        }
    }

    fn item(library: &[u8], key: &str, extra: serde_json::Value) -> serde_json::Value {
        let mut item = json!({"item":{"library":LibraryId::digest(library),"key":key}});
        item.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        item
    }

    fn cite_items(items: Vec<serde_json::Value>, form: &str) -> CitationResult {
        let request: CitationRequest = serde_json::from_value(
            json!({"items":items,"form":form,"output":{"kind":"key","syntax":"scannable-cite"}}),
        )
        .unwrap();
        super::cite(&snapshot(), &request, origin).unwrap()
    }

    fn text(items: Vec<serde_json::Value>) -> String {
        let result = cite_items(items, "normal");
        assert!(result.copy_content(result.outcomes.len()).is_some(), "{:?}", result.outcomes);
        result.text
    }

    fn refusal(items: Vec<serde_json::Value>, form: &str) -> Vec<CitationError> {
        let result = cite_items(items, form);
        assert_eq!(result.text, "", "nothing is written when anything is refused");
        result
            .outcomes
            .iter()
            .filter_map(|outcome| match outcome {
                ItemOutcome::Refused { error, .. } => Some(*error),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_marker_names_the_item_in_its_zotero_library_with_every_part() {
        assert_eq!(
            text(vec![item(PERSONAL, "ABCD2345", json!({}))]),
            "{ | Example, 2026 | | | zu:0:ABCD2345 }"
        );
        assert_eq!(
            text(vec![item(
                PERSONAL,
                "ABCD2345",
                json!({
                    "prefix":[{"text":"See"}],
                    "locator":{"term":"page","value":" 12 "},
                    "suffix":[{"text":"for "},{"text":"comparison","italic":true},{"text":", "},{"text":"twice","bold":true}],
                })
            )]),
            "{ See | Example, 2026 | p. 12 | for *comparison*, **twice** | zu:0:ABCD2345 }"
        );
        // A group library is named by its group ID; text the label cannot
        // carry is dropped, as the label is display only.
        assert_eq!(
            text(vec![
                item(PERSONAL, "ABCD2345", json!({"suppress_author":true,"locator":{"term":"chapter","value":"3"}})),
                item(GROUP, "WXYZ6789", json!({})),
            ]),
            "{ | -Example, 2026 | ch. 3 | | zu:0:ABCD2345 }{ | Untitled braced atlas, 2025 | | | zg:4321:WXYZ6789 }"
        );
    }

    #[test]
    fn anything_a_marker_cannot_say_refuses_the_whole_citation() {
        let good = || item(PERSONAL, "ABCD2345", json!({}));
        for (extra, expected) in [
            (json!({"locator":{"term":"timestamp","value":"1:02"}}), CitationError::UnsupportedForm),
            (json!({"locator":{"term":"page","value":"1|2"}}), CitationError::InvalidLocator),
            (json!({"prefix":[{"text":"see | also"}]}), CitationError::InvalidRichText),
            (json!({"suffix":[{"text":"{x}"}]}), CitationError::InvalidRichText),
            (json!({"suffix":[{"text":"a*b"}]}), CitationError::InvalidRichText),
            (json!({"suffix":[{"text":"one","italic":true},{"text":" and "},{"text":"two","italic":true}]}), CitationError::InvalidRichText),
            (json!({"suffix":[{"text":"both","italic":true,"bold":true}]}), CitationError::InvalidRichText),
            (json!({"suffix":[{"text":"2","script":"superscript"}]}), CitationError::InvalidRichText),
            (json!({"form":"narrative"}), CitationError::UnsupportedForm),
            (json!({"expected_cite":"0".repeat(64)}), CitationError::ChangedItem),
        ] {
            assert_eq!(refusal(vec![good(), item(PERSONAL, "ABCD2345", extra.clone())], "normal"), [expected], "{extra}");
        }
        // Not read from Zotero, a key Zotero never makes, or not in the library at all.
        assert_eq!(refusal(vec![item(b"bibtex-file", "example2026", json!({}))], "normal"), [CitationError::NotFromZotero]);
        assert_eq!(refusal(vec![item(PERSONAL, "lowercase", json!({}))], "normal"), [CitationError::NotFromZotero]);
        assert_eq!(refusal(vec![item(PERSONAL, "MISSING2", json!({}))], "normal"), [CitationError::MissingItem]);
        // A legal item takes no locator; without one it is cited.
        assert_eq!(
            refusal(vec![item(PERSONAL, "LEGAL234", json!({"locator":{"term":"page","value":"4"}}))], "normal"),
            [CitationError::UnsupportedForm]
        );
        assert_eq!(text(vec![item(PERSONAL, "LEGAL234", json!({}))]), "{ | Invented v. Example | | | zu:0:LEGAL234 }");
        for form in ["narrative", "author-only", "year-only", "full", "bibliography-only"] {
            assert_eq!(refusal(vec![good()], form), [CitationError::UnsupportedForm], "{form}");
        }
    }

    #[test]
    fn the_key_path_never_writes_a_marker() {
        let request: CitationRequest = serde_json::from_value(
            json!({"items":[item(PERSONAL, "ABCD2345", json!({}))],"output":{"kind":"key","syntax":"scannable-cite"}}),
        )
        .unwrap();
        assert_eq!(super::super::cite(&snapshot(), &request), Err(CitationError::UnsupportedOutput));
    }
}
