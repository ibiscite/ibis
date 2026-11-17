//! A reference's attachments and their text, as a source knows them (decision
//! 0055). Locations stay in the source: what leaves it is a key the source
//! gave, a title and a media type, and text held in memory for one call.
use ibis_core::contract::Fingerprint;

use crate::pdf::{PdfDocument, PdfReader, Refusal};
use std::path::Path;
use std::sync::Arc;

/// Attachments listed for one reference at most, from every source.
pub const MAX_ATTACHMENTS: usize = crate::zotero_local::fulltext::MAX_ATTACHMENTS;

/// One attachment of a reference. Its title is library text.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceAttachment {
    /// The source's own name for it, never a location.
    pub key: String,
    /// Opaque check of the source's attachment metadata, including its revision.
    pub revision: Fingerprint,
    pub title: String,
    pub media_type: String,
    /// A link to a web page, with no file and so no text.
    pub link_only: bool,
    /// Where the source keeps the file, in the source's own terms; it never
    /// leaves the source.
    pub(crate) location: String,
}
impl SourceAttachment {
    pub fn new(key: String, revision: Fingerprint, title: String, media_type: String, link_only: bool) -> Self {
        Self {
            key,
            revision,
            title,
            media_type,
            link_only,
            location: String::new(),
        }
    }

    /// A PDF, by the type its library gives it.
    pub fn is_pdf(&self) -> bool {
        self.media_type.eq_ignore_ascii_case("application/pdf")
    }
}
impl std::fmt::Debug for SourceAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SourceAttachment { private }")
    }
}

/// What a source can give of one attachment's text.
#[derive(Clone, PartialEq, Eq)]
pub enum TextState {
    Text(DocumentText),
    /// The source holds no text for it: Zotero has not indexed it.
    NotIndexed,
    /// A link, with no file.
    NoFile,
    /// A file whose text needs a reader Ibis does not have yet.
    NotYet,
    /// A PDF whose file Ibis would not or could not read.
    Refused(Refusal),
}
impl std::fmt::Debug for TextState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TextState { private }")
    }
}

/// An attachment's text. Pages are known only when their breaks were
/// verified; a page number is then the file's own (physical) page, never a
/// printed label. Printed labels come only from the PDF's `/PageLabels`.
#[derive(Clone, PartialEq, Eq)]
pub struct DocumentText {
    /// Opaque check of the text, index metadata and any PDF used for text or labels.
    pub revision: Fingerprint,
    pub body: Body,
    /// Pages the source read, from the first, when it says.
    pub indexed_pages: Option<u32>,
    pub total_pages: Option<u32>,
    /// Each page's printed label, when `body` has pages and the PDF's label
    /// tree gave one label slot for each of them; a page without a citable
    /// label is `None`.
    pub cite_pages: Option<Vec<Option<String>>>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Body {
    /// Page 1 first.
    Pages(Vec<String>),
    /// Where pages begin and end is not known.
    Whole(String),
}

impl DocumentText {
    /// Text Zotero indexed, with its pages when its breaks match the pages
    /// Zotero recorded.
    pub fn from_zotero(text: String, indexed_pages: Option<u32>, total_pages: Option<u32>) -> Self {
        let content = Fingerprint::of_bytes(text.as_bytes());
        let revision =
            Fingerprint::of_bytes(format!("{}:{indexed_pages:?}:{total_pages:?}", content.as_str()).as_bytes());
        let verified = match (indexed_pages, total_pages) {
            (Some(indexed), Some(total)) if indexed == total => {
                crate::zotero_local::fulltext::pages(&text, Some(indexed))
            }
            _ => None,
        };
        let body = match verified {
            Some(pages) => Body::Pages(pages.into_iter().map(str::to_owned).collect()),
            None => Body::Whole(text),
        };
        Self {
            revision,
            body,
            indexed_pages,
            total_pages,
            cite_pages: None,
        }
    }

    /// A PDF's own text, page by page, with its labels.
    pub fn from_pdf(document: PdfDocument) -> Self {
        Self {
            revision: document.revision,
            body: Body::Pages(document.text.unwrap_or_default()),
            indexed_pages: Some(document.pages),
            total_pages: Some(document.pages),
            cite_pages: document.labels,
        }
    }

    /// Zotero's pages are trusted and cover the whole file, as it recorded.
    fn complete(&self) -> bool {
        matches!(self.body, Body::Pages(_)) && self.total_pages.is_none_or(|total| self.indexed_pages == Some(total))
    }

    /// The same text with the PDF's labels, only when the PDF has exactly
    /// as many pages as this text.
    fn with_labels(mut self, document: &PdfDocument) -> Self {
        self.revision =
            Fingerprint::of_bytes(format!("{}:{}", self.revision.as_str(), document.revision.as_str()).as_bytes());
        if let Body::Pages(pages) = &self.body
            && u32::try_from(pages.len()) == Ok(document.pages)
            && self.total_pages.is_none_or(|total| total == document.pages)
        {
            self.cite_pages = document.labels.clone();
        }
        self
    }

    /// The printed label of the file's page `pdf_page`, counted from 1.
    pub fn cite_page(&self, pdf_page: u32) -> Option<String> {
        let index = usize::try_from(pdf_page.checked_sub(1)?).ok()?;
        self.cite_pages.as_ref()?.get(index)?.clone()
    }
}

/// An attachment's text from Zotero's index, the PDF itself, or both.
///
/// Zotero's text wins while its pages are trusted and cover the file; the PDF
/// then gives only its labels, which count when its page count matches
/// exactly. Otherwise the PDF's own text is read, and Zotero's partial or
/// unpaged text is kept only when the PDF cannot be read.
pub fn text_of(
    zotero: Option<DocumentText>,
    pdf: Option<(&PdfReader, &Path)>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> TextState {
    let Some((reader, file)) = pdf else {
        return zotero.map_or(TextState::NotIndexed, TextState::Text);
    };
    match zotero {
        Some(text) if text.complete() => TextState::Text(match reader.read(file, true, cancelled) {
            Ok(document) => text.with_labels(&document),
            Err(_) => text,
        }),
        zotero => match reader.read(file, false, cancelled) {
            Ok(document) => {
                let mut text = DocumentText::from_pdf(document);
                if let Some(indexed) = zotero {
                    text.revision = Fingerprint::of_bytes(
                        format!("{}:{}", indexed.revision.as_str(), text.revision.as_str()).as_bytes(),
                    );
                }
                TextState::Text(text)
            }
            Err(refusal) => zotero.map_or(TextState::Refused(refusal), TextState::Text),
        },
    }
}

#[cfg(test)]
#[cfg(unix)]
mod pdf_tests {
    use super::*;

    /// A stand-in for `ibis __extract-pdf` that answers as the real child
    /// would for an invented two-page PDF labelled i and ii.
    fn reader(folder: &Path) -> PdfReader {
        use std::os::unix::fs::PermissionsExt;
        let program = folder.join("invented-ibis");
        std::fs::write(
            &program,
            "#!/bin/sh\ncat >/dev/null\nif [ \"$2\" = --labels-only ]; then\n\
             printf '{\"read\":{\"pages\":2,\"labels\":[\"i\",\"ii\"],\"text\":null}}'\nelse\n\
             printf '{\"read\":{\"pages\":2,\"labels\":[\"i\",\"ii\"],\"text\":[\"PDF one\",\"PDF two\"]}}'\nfi\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        PdfReader::new(program)
    }

    #[test]
    fn zoteros_complete_pages_win_and_take_labels_only_when_page_counts_match() {
        let folder = tempfile::tempdir().unwrap();
        let root = crate::zotero_local::closed::trusted_folder(folder.path());
        let reader = reader(&root);
        let file = root.join("invented.pdf");
        std::fs::write(&file, "%PDF-1.7\n").unwrap();
        let pdf = Some((&reader, file.as_path()));

        let complete = DocumentText::from_zotero("Zotero one\u{c}Zotero two".into(), Some(2), Some(2));
        let TextState::Text(text) = text_of(Some(complete), pdf, Arc::new(|| false)) else {
            panic!("text")
        };
        assert!(text.body == Body::Pages(vec!["Zotero one".into(), "Zotero two".into()]));
        assert_eq!(
            (text.cite_page(1), text.cite_page(2)),
            (Some("i".into()), Some("ii".into()))
        );

        // Zotero counted three pages, the PDF two: no labels, never shifted ones.
        let other = DocumentText::from_zotero("One\u{c}Two\u{c}Three".into(), Some(3), Some(3));
        let TextState::Text(text) = text_of(Some(other), pdf, Arc::new(|| false)) else {
            panic!("text")
        };
        assert_eq!(text.cite_pages, None);

        // Even when Zotero's text wins, a changed PDF retires its labels and id.
        let before = text.revision.clone();
        std::fs::write(&file, "%PDF-1.7\nreplacement\n").unwrap();
        let other = DocumentText::from_zotero("One\u{c}Two\u{c}Three".into(), Some(3), Some(3));
        let TextState::Text(changed) = text_of(Some(other), pdf, Arc::new(|| false)) else {
            panic!("text")
        };
        assert_ne!(changed.revision, before);

        // A partial index gives way to the PDF's own pages.
        let partial = DocumentText::from_zotero("Zotero one".into(), Some(1), Some(2));
        let TextState::Text(text) = text_of(Some(partial.clone()), pdf, Arc::new(|| false)) else {
            panic!("text")
        };
        assert!(text.body == Body::Pages(vec!["PDF one".into(), "PDF two".into()]));
        assert_eq!(text.cite_page(2), Some("ii".into()));

        // When the PDF cannot be read, Zotero's partial text is still given.
        let gone = root.join("gone.pdf");
        assert!(
            text_of(
                Some(partial.clone()),
                Some((&reader, gone.as_path())),
                Arc::new(|| false)
            ) == TextState::Text(partial)
        );
        assert!(
            text_of(None, Some((&reader, gone.as_path())), Arc::new(|| false)) == TextState::Refused(Refusal::Missing)
        );
        assert!(text_of(None, None, Arc::new(|| false)) == TextState::NotIndexed);
    }
}
