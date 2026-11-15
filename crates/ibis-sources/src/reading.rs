//! A reference's attachments and their text, as a source knows them (decision
//! 0055). Locations stay in the source: what leaves it is a key the source
//! gave, a title and a media type, and text held in memory for one call.

/// Attachments listed for one reference at most, from every source.
pub const MAX_ATTACHMENTS: usize = ibis_zotero_local::fulltext::MAX_ATTACHMENTS;

/// One attachment of a reference. Its title is library text.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceAttachment {
    /// The source's own name for it, never a location.
    pub key: String,
    pub title: String,
    pub media_type: String,
    /// A link to a web page, with no file and so no text.
    pub link_only: bool,
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
}
impl std::fmt::Debug for TextState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TextState { private }")
    }
}

/// An attachment's text. Pages are known only when their breaks were
/// verified; a page number is then the file's own (physical) page, never a
/// printed label.
#[derive(Clone, PartialEq, Eq)]
pub struct DocumentText {
    pub body: Body,
    /// Pages the source read, from the first, when it says.
    pub indexed_pages: Option<u32>,
    pub total_pages: Option<u32>,
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
        let body = match ibis_zotero_local::fulltext::pages(&text, indexed_pages) {
            Some(pages) => Body::Pages(pages.into_iter().map(str::to_owned).collect()),
            None => Body::Whole(text),
        };
        Self {
            body,
            indexed_pages,
            total_pages,
        }
    }
}
