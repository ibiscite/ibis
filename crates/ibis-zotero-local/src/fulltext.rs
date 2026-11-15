//! The text Zotero indexed for an attachment (decision 0055).
//!
//! Zotero keeps it in `.zotero-ft-cache` in the attachment's storage folder and
//! records how many pages it indexed. While Zotero runs, its local API answers
//! with both at `items/KEY/fulltext`; while it is closed, the private database
//! copy gives the counts and [`crate::closed::read_text_cache`] the file.
//!
//! Zotero's PDF worker (`getFulltext` in zotero/pdf-worker `src/pdf/index.js`)
//! ends every page with a blank line and puts a form feed between pages, then
//! trims the whole text. Older Zotero, through pdftotext, ended each page with
//! a form feed. Neither is proof on its own: a trimmed empty first or last page
//! moves every break. So page breaks are trusted only when they separate exactly
//! the number of pages Zotero recorded; otherwise the text has no pages.

use ibis_core::zotero::Library;
use ibis_core::zotero::api::{Transport, TransportError};
use serde_json::Value;

/// The most text one attachment may hold for Ibis to read it.
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
/// The most attachments of one item Ibis lists.
pub const MAX_ATTACHMENTS: usize = 50;

/// What Zotero indexed for one attachment.
#[derive(Clone, PartialEq, Eq)]
pub struct Indexed {
    pub text: String,
    /// Pages Zotero read, from its first page.
    pub indexed_pages: Option<u32>,
    pub total_pages: Option<u32>,
}
impl std::fmt::Debug for Indexed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Indexed { private }")
    }
}

/// One attachment of an item, as Zotero lists it. Its title is library text.
#[derive(Clone, PartialEq, Eq)]
pub struct Attachment {
    pub key: String,
    pub title: String,
    pub content_type: String,
    /// A link to a web page, with no file and no text.
    pub link_only: bool,
}
impl std::fmt::Debug for Attachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Attachment { private }")
    }
}

/// Fixed reasons only: no text, key or path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    Transport(TransportError),
    /// Zotero answered, but not as its local API does.
    Malformed,
    TooLarge,
}

/// The pages of `text`, first to last, when its page breaks separate exactly
/// the `indexed` pages Zotero recorded.
pub fn pages(text: &str, indexed: Option<u32>) -> Option<Vec<&str>> {
    let indexed = usize::try_from(indexed?).ok().filter(|count| *count > 0)?;
    // pdftotext ended the last page with a form feed too.
    let text = text.strip_suffix('\u{c}').unwrap_or(text);
    let pages: Vec<&str> = text.split('\u{c}').collect();
    (pages.len() == indexed).then_some(pages)
}

/// A valid Zotero key: eight capital letters and digits.
pub fn valid_key(key: &str) -> bool {
    key.len() == 8 && key.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

fn prefix(library: Library) -> String {
    match library {
        Library::User => "users/0".into(),
        Library::Group(id) => format!("groups/{id}"),
    }
}

/// The attachments of `parent`, from Zotero's local API, in Zotero's order.
pub fn attachments(
    transport: &mut impl Transport,
    library: Library,
    parent: &str,
) -> Result<Vec<Attachment>, TextError> {
    if !valid_key(parent) {
        return Err(TextError::Malformed);
    }
    let response = transport
        .get(&format!(
            "{}/items/{parent}/children?itemType=attachment&limit={MAX_ATTACHMENTS}",
            prefix(library)
        ))
        .map_err(TextError::Transport)?;
    if response.status != 200 || response.header("Zotero-API-Version") != Some("3") {
        return Err(TextError::Malformed);
    }
    let rows: Vec<Value> = serde_json::from_slice(&response.body).map_err(|_| TextError::Malformed)?;
    Ok(rows
        .iter()
        .take(MAX_ATTACHMENTS)
        .filter_map(|row| {
            let data = row.get("data")?;
            let key = row.get("key")?.as_str()?;
            if data.get("itemType")?.as_str()? != "attachment"
                || data.get("parentItem")?.as_str()? != parent
                || data.get("key").and_then(Value::as_str) != Some(key)
                || !valid_key(key)
            {
                return None;
            }
            let text = |name: &str| {
                data.get(name)
                    .and_then(Value::as_str)
                    .filter(|text| text.len() <= 1024)
                    .unwrap_or_default()
                    .to_owned()
            };
            Some(Attachment {
                key: key.to_owned(),
                title: text("title"),
                content_type: text("contentType"),
                link_only: data.get("linkMode").and_then(Value::as_str) == Some("linked_url"),
            })
        })
        .collect())
}

/// What Zotero indexed for the attachment `key`, or `None` when it holds no
/// text for it. Zotero releases without this endpoint also answer 404.
pub fn indexed(transport: &mut impl Transport, library: Library, key: &str) -> Result<Option<Indexed>, TextError> {
    if !valid_key(key) {
        return Err(TextError::Malformed);
    }
    let response = transport
        .get(&format!("{}/items/{key}/fulltext", prefix(library)))
        .map_err(TextError::Transport)?;
    if response.status == 404 {
        return Ok(None);
    }
    if response.status != 200 || response.header("Zotero-API-Version") != Some("3") {
        return Err(TextError::Malformed);
    }
    let answer: Value = serde_json::from_slice(&response.body).map_err(|_| TextError::Malformed)?;
    let text = answer
        .get("content")
        .and_then(Value::as_str)
        .ok_or(TextError::Malformed)?;
    if text.len() > MAX_TEXT_BYTES {
        return Err(TextError::TooLarge);
    }
    let count = |name: &str| match answer.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|count| u32::try_from(count).ok())
            .map(Some)
            .ok_or(TextError::Malformed),
    };
    Ok(Some(Indexed {
        text: text.to_owned(),
        indexed_pages: count("indexedPages")?,
        total_pages: count("totalPages")?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::zotero::api::Response;

    #[test]
    fn page_breaks_count_only_when_they_match_zoteros_record() {
        let text = "First invented page.\n\n\u{c}Second invented page.\n\n\u{c}Third.";
        assert_eq!(
            pages(text, Some(3)),
            Some(vec!["First invented page.\n\n", "Second invented page.\n\n", "Third."])
        );
        // A trimmed empty page, or a page Zotero did not count, moves every break.
        assert_eq!(pages(text, Some(4)), None);
        assert_eq!(pages(text, Some(2)), None);
        assert_eq!(pages(text, None), None);
        assert_eq!(pages(text, Some(0)), None);
        // pdftotext's closing form feed is one page end, not another page.
        assert_eq!(pages("One\u{c}Two\u{c}", Some(2)), Some(vec!["One", "Two"]));
        assert_eq!(pages("One\u{c}Two\u{c}\u{c}", Some(2)), None);
        assert_eq!(pages("Only page", Some(1)), Some(vec!["Only page"]));
    }

    struct Fake(Vec<(&'static str, u16, String)>);
    impl Transport for Fake {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            let (wanted, status, body) = self.0.remove(0);
            assert_eq!(path, wanted);
            Ok(Response {
                status,
                headers: vec![("Zotero-API-Version".into(), "3".into())],
                body: body.into_bytes(),
            })
        }
    }

    #[test]
    fn attachments_belong_to_the_parent_and_fulltext_gives_counts() {
        let rows = serde_json::json!([
            {"key":"PDF00001","data":{"key":"PDF00001","itemType":"attachment","parentItem":"SYNTH001","title":"Invented PDF","contentType":"application/pdf","linkMode":"imported_file"}},
            {"key":"LINK0001","data":{"key":"LINK0001","itemType":"attachment","parentItem":"SYNTH001","linkMode":"linked_url"}},
            {"key":"OTHER001","data":{"key":"OTHER001","itemType":"attachment","parentItem":"SYNTH999"}},
            {"key":"NOTE0001","data":{"key":"NOTE0001","itemType":"note","parentItem":"SYNTH001"}}
        ]);
        let mut fake = Fake(vec![(
            "groups/7/items/SYNTH001/children?itemType=attachment&limit=50",
            200,
            rows.to_string(),
        )]);
        let found = attachments(&mut fake, Library::Group(7), "SYNTH001").unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].title, "Invented PDF");
        assert!(!found[0].link_only && found[1].link_only);

        let mut fake = Fake(vec![
            (
                "users/0/items/PDF00001/fulltext",
                200,
                serde_json::json!({"content":"One\u{c}Two","indexedPages":2,"totalPages":9}).to_string(),
            ),
            ("users/0/items/PDF00002/fulltext", 404, String::new()),
            ("users/0/items/PDF00003/fulltext", 200, r#"{"content":7}"#.into()),
        ]);
        let text = indexed(&mut fake, Library::User, "PDF00001").unwrap().unwrap();
        assert_eq!((text.indexed_pages, text.total_pages), (Some(2), Some(9)));
        assert_eq!(indexed(&mut fake, Library::User, "PDF00002"), Ok(None));
        assert_eq!(indexed(&mut fake, Library::User, "PDF00003"), Err(TextError::Malformed));
        assert_eq!(
            indexed(&mut fake, Library::User, "../x"),
            Err(TextError::Malformed),
            "a key never reaches a request path unchecked"
        );
    }
}
