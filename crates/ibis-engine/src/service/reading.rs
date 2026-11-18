//! Reading a reference's attachments (decision 0055): what they are, a few of
//! their pages, and short passages that hold some words.
//!
//! Text comes from the source, held in memory for one call; nothing is written.
//! Zotero's index gives it where it has one; a PDF is otherwise read by the
//! extractor's child process, an `ibis` executable this engine was given, and
//! without one PDFs stay unread. An attachment id names one attachment of one
//! reference while its library generation, attachment, index and PDF stay
//! unchanged; it never holds a path or file URL. A page number is the file's
//! own page, and only when its
//! breaks were verified; `cite_page` comes only from the PDF's `/PageLabels`,
//! never from the file's page number.
use super::*;
use crate::request::{FindInRequest, OutlineRequest, ReadRequest};
use crate::sync::MutexExt;
use ibis_sources::pdf::{PdfReader, Refusal};
use ibis_sources::reading::{Body, DocumentText, SourceAttachment, TextState};
use unicode_normalization::UnicodeNormalization;

/// Pages one `read` returns at most.
pub const MAX_READ_PAGES: u32 = 5;
/// Bytes of text one `read` returns at most.
pub const MAX_READ_BYTES: usize = 32 * 1024;
/// Passages one `find-in` returns at most.
pub const MAX_PASSAGES: usize = 10;
/// Characters in one passage, about.
pub const PASSAGE_CHARS: usize = 400;
const MAX_QUERY_CHARS: usize = 200;
const ID_MARK: &str = "att:";
const SLOT_LENGTH: usize = 16;

/// A reference's attachments, in its library's order.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OutlineResult {
    pub attachments: Vec<AttachmentOutline>,
}

/// One attachment, without its text.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AttachmentOutline {
    /// Pass to `read` or `find-in` in this library generation, while the
    /// reference, attachment revision and indexed text remain unchanged.
    pub id: String,
    /// The attachment's title as safe bibliographic text, omitting private locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub text: TextAvailability,
    /// Pages the file has, as its source recorded them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<u32>,
    /// Pages that have text, from the first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages_with_text: Option<u32>,
    /// The text's page breaks were verified, so each page has its number.
    pub pages_known: bool,
    /// Printed page labels are known, so pages have a `cite_page`.
    pub cite_pages_known: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TextAvailability {
    /// Its text can be read.
    Available,
    /// The library holds no text for it: Zotero has not indexed it.
    NotIndexed,
    /// A link, with no file.
    NoFile,
    /// A file Ibis does not read: not a PDF, or a PDF where no PDF reader runs
    /// (only `ibis` reads PDFs, never the desktop app).
    NotYet,
    /// Text larger than Ibis reads at once, or a PDF over 256 MiB or 10,000 pages.
    TooLarge,
    /// No file is where the library says it is.
    Missing,
    /// Not a plain file on this computer: a link, a network or online-only
    /// file, or one Ibis may not open.
    NotLocal,
    /// The file is not a PDF.
    NotPdf,
    /// The PDF is damaged or could not be parsed.
    Damaged,
    /// Reading the PDF took longer than ten seconds.
    TimedOut,
}

impl From<Refusal> for TextAvailability {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::Missing => Self::Missing,
            Refusal::NotLocal => Self::NotLocal,
            Refusal::NotPdf => Self::NotPdf,
            Refusal::TooLarge => Self::TooLarge,
            Refusal::Damaged => Self::Damaged,
            Refusal::TimedOut => Self::TimedOut,
            Refusal::Unavailable => Self::NotYet,
        }
    }
}

/// Some of an attachment's text, page by page.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ReadResult {
    pub attachment: String,
    pub pages: Vec<PageText>,
    pub pages_known: bool,
    /// The arguments for the next `read`, while text remains.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<ReadNext>,
}

/// One page's text, or the text read when pages are not known.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PageText {
    /// The file's own page number, from 1; null when pages are not known.
    pub pdf_page: Option<u32>,
    /// The printed page to cite, only from verified labels; null otherwise.
    pub cite_page: Option<String>,
    pub text: String,
    /// The page holds more than one read returns; `next` goes on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ReadNext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

/// Passages that hold the words, in the order they appear.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FindInResult {
    pub attachment: String,
    pub passages: Vec<Passage>,
    /// False when more passages hold the words than one answer gives.
    pub complete: bool,
    pub pages_known: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Passage {
    /// The file's own page number, from 1; null when pages are not known.
    pub pdf_page: Option<u32>,
    /// The printed page to cite, only from verified labels; null otherwise.
    pub cite_page: Option<String>,
    /// About 400 characters around the words, on one line.
    pub text: String,
}

/// Where a reference's attachments come from.
pub(super) enum Owner {
    Zotero(ibis_sources::zotero_source::ZoteroReading, u16),
    /// A library file, whose attachments are found from its folder.
    File(std::path::PathBuf),
    Snapshot,
}

impl Engine {
    /// The attachments of one reference, and whether their text can be read.
    pub fn outline(
        &self,
        request: &OutlineRequest,
        cancel: &Cancellation,
    ) -> Result<health::Checked<OutlineResult>, EngineError> {
        cancel.check()?;
        let state = self.state()?;
        let parent = select(state.index.snapshot(), &state.libraries, &request.item)?.clone();
        if !matches!(parent.object, LibraryObject::Record(_)) {
            return Err(error(ErrorCode::NotFound));
        }
        let owner = self.owner(&state, &parent)?;
        let generation = self.attachment_generation(&state, &parent.object.id().library)?;
        let mut attachments = Vec::new();
        for attachment in self.listed(&state, &owner, &parent)? {
            cancel.check()?;
            // One text too large to read leaves the others to list.
            let text = match self.text(&owner, &parent, &attachment, cancel) {
                Err(failure) if failure.code == ErrorCode::ResourceLimit => None,
                text => Some(text?),
            };
            let cite_pages_known = matches!(&text, Some(TextState::Text(text)) if text.cite_pages.is_some());
            let id = attachment_id(&parent, &attachment, generation, text.as_ref());
            let (availability, pages, pages_with_text, pages_known) = match &text {
                None => (TextAvailability::TooLarge, None, None, false),
                Some(TextState::Text(text)) => (
                    TextAvailability::Available,
                    text.total_pages,
                    match &text.body {
                        Body::Pages(pages) => u32::try_from(pages.len()).ok(),
                        Body::Whole(_) => text.indexed_pages,
                    },
                    matches!(text.body, Body::Pages(_)),
                ),
                Some(TextState::NotIndexed) => (TextAvailability::NotIndexed, None, None, false),
                Some(TextState::NoFile) => (TextAvailability::NoFile, None, None, false),
                Some(TextState::NotYet) => (TextAvailability::NotYet, None, None, false),
                Some(TextState::Refused(refusal)) => ((*refusal).into(), None, None, false),
            };
            let projected = ibis_core::citable::CitableFields::new(serde_json::Map::from_iter([(
                "title".into(),
                json!(attachment.title),
            )]));
            let title = projected
                .plain()
                .get("title")
                .and_then(Value::as_str)
                .map(|title| title.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|title| !title.is_empty());
            attachments.push(AttachmentOutline {
                id,
                title,
                media_type: ibis_core::model::is_media_type(&attachment.media_type).then_some(attachment.media_type),
                text: availability,
                pages,
                pages_with_text,
                pages_known,
                cite_pages_known,
            });
        }
        let result = OutlineResult { attachments };
        cancel.check()?;
        let current = self.state()?;
        if !state.same_publication(&current)
            || generation != self.attachment_generation(&current, &parent.object.id().library)?
        {
            return Err(error(ErrorCode::NotFound));
        }
        ensure_budget(&result)?;
        Ok(health::Checked {
            source_health: consulted(&state, &request.item, &parent.object.id().library),
            value: result,
        })
    }

    /// At most five pages or 32 KiB of an attachment's text.
    pub fn read_attachment(
        &self,
        request: &ReadRequest,
        cancel: &Cancellation,
    ) -> Result<health::Checked<ReadResult>, EngineError> {
        let (state, parent, text) = self.attachment_text(&request.attachment, cancel)?;
        let (pages, next) = read_pages(&text, request)?;
        let result = ReadResult {
            attachment: request.attachment.clone(),
            pages_known: matches!(text.body, Body::Pages(_)),
            pages,
            next,
        };
        ensure_budget(&result)?;
        Ok(health::Checked {
            source_health: own_health(&state, &parent),
            value: result,
        })
    }

    /// At most ten short passages of an attachment that hold `query`.
    pub fn find_in(
        &self,
        request: &FindInRequest,
        cancel: &Cancellation,
    ) -> Result<health::Checked<FindInResult>, EngineError> {
        let query = fold(request.query.trim()).0;
        if query.trim().is_empty() || request.query.chars().count() > MAX_QUERY_CHARS {
            return Err(error(ErrorCode::BadInput));
        }
        let (state, parent, text) = self.attachment_text(&request.attachment, cancel)?;
        let (passages, complete) = find_passages(&text, &query, cancel)?;
        let result = FindInResult {
            attachment: request.attachment.clone(),
            passages,
            complete,
            pages_known: matches!(text.body, Body::Pages(_)),
        };
        ensure_budget(&result)?;
        Ok(health::Checked {
            source_health: own_health(&state, &parent),
            value: result,
        })
    }

    /// The library of the reference an attachment id belongs to.
    pub fn attachment_library(&self, attachment: &str) -> Result<LibraryId, EngineError> {
        let state = self.state()?;
        let (parent, _) = parse_id(&state, attachment)?;
        Ok(parent.object.id().library.clone())
    }

    /// Whether the person let AI apps read `library`'s attachment text.
    pub fn attachment_text_allowed(
        &self,
        settings: &crate::settings::Settings,
        library: &LibraryId,
    ) -> Result<bool, EngineError> {
        let state = self.state()?;
        Ok(state
            .libraries
            .get(library)
            .is_some_and(|info| info.capabilities.contains(&SourceCapability::AttachmentText))
            && settings.attachment_text(library))
    }

    pub(super) fn owner(&self, state: &State, parent: &SnapshotObject) -> Result<Owner, EngineError> {
        let kind = state
            .libraries
            .get(&parent.object.id().library)
            .map(|info| info.kind)
            .ok_or_else(|| error(ErrorCode::NotFound))?;
        Ok(match kind {
            LibraryKind::Zotero => {
                let (reading, port) = self.zotero_reading()?;
                Owner::Zotero(reading, port)
            }
            LibraryKind::File => match self.file_path(&parent.object.id().library)? {
                Some(path) => Owner::File(path),
                None => Owner::Snapshot,
            },
            LibraryKind::Snapshot => Owner::Snapshot,
        })
    }

    fn attachment_generation(&self, state: &State, library: &LibraryId) -> Result<u64, EngineError> {
        Ok(self
            .shared
            .sources
            .recover_lock()
            .file_generation(library)
            .unwrap_or(state.zotero_generation))
    }

    pub(super) fn listed(
        &self,
        state: &State,
        owner: &Owner,
        parent: &SnapshotObject,
    ) -> Result<Vec<SourceAttachment>, EngineError> {
        let id = parent.object.id();
        let found = match owner {
            Owner::Zotero(reading, port) => reading
                .attachments(&mut ibis_sources::zotero_source::LocalZotero::on_port(*port), id)
                .map_err(sources::zotero_error)?,
            Owner::File(_) | Owner::Snapshot => state
                .index
                .snapshot()
                .objects()
                .iter()
                .filter_map(|item| match &item.object {
                    LibraryObject::Attachment(attachment) if attachment.parent.as_ref() == Some(id) => {
                        Some(SourceAttachment::new(
                            attachment.id.key.clone(),
                            item.fingerprints.full.clone(),
                            attachment.title.clone(),
                            attachment.media_type.clone(),
                            attachment.availability == Availability::Online,
                        ))
                    }
                    _ => None,
                })
                .take(ibis_sources::reading::MAX_ATTACHMENTS)
                .collect(),
        };
        Ok(found)
    }

    fn text(
        &self,
        owner: &Owner,
        parent: &SnapshotObject,
        attachment: &SourceAttachment,
        cancel: &Cancellation,
    ) -> Result<TextState, EngineError> {
        let mut text = match owner {
            Owner::Zotero(reading, port) => reading
                .text(
                    &mut ibis_sources::zotero_source::LocalZotero::on_port(*port),
                    parent.object.id(),
                    attachment,
                    self.pdf.as_ref(),
                    self.attachment_text_outside_library,
                    {
                        let cancel = cancel.clone();
                        Arc::new(move || cancel.is_cancelled())
                    },
                )
                .map_err(sources::zotero_error),
            _ if attachment.link_only => Ok(TextState::NoFile),
            Owner::File(library) => match self.pdf.as_ref().filter(|_| attachment.is_pdf()) {
                Some(reader) => file_text(
                    reader,
                    library,
                    parent,
                    attachment,
                    self.attachment_text_outside_library,
                    cancel,
                ),
                None => Ok(TextState::NotYet),
            },
            Owner::Snapshot => Ok(TextState::NotYet),
        }?;
        if let TextState::Text(text) = &mut text
            && let LibraryObject::Record(record) = &parent.object
            && let Some(pages) = record.csl.get("page")
            && !pages.as_str().is_some_and(|pages| pages.trim().is_empty())
        {
            // All three methods see the same filtered labels. Unconfirmed or
            // unrecognized ranges cannot authorize a citation (0036).
            text.cite_pages = text.cite_pages.take().and_then(|mut labels| {
                for label in &mut labels {
                    if label
                        .as_ref()
                        .is_some_and(|label| !label_in_reference_pages(label, pages))
                    {
                        *label = None;
                    }
                }
                labels.iter().any(Option::is_some).then_some(labels)
            });
        }
        Ok(text)
    }

    /// The text of the attachment an id names, refused when there is none.
    fn attachment_text(
        &self,
        attachment: &str,
        cancel: &Cancellation,
    ) -> Result<(Arc<State>, SnapshotObject, DocumentText), EngineError> {
        cancel.check()?;
        let state = self.state()?;
        let (parent, check) = parse_id(&state, attachment)?;
        let generation = self.attachment_generation(&state, &parent.object.id().library)?;
        let (slot, _) = check.split_once('.').ok_or_else(|| error(ErrorCode::NotFound))?;
        let owner = self.owner(&state, &parent)?;
        let listed = self
            .listed(&state, &owner, &parent)?
            .into_iter()
            .find(|listed| attachment_slot(&parent, listed, generation) == slot)
            .ok_or_else(|| error(ErrorCode::NotFound))?;
        cancel.check()?;
        let text = self.text(&owner, &parent, &listed, cancel)?;
        let current = self.state()?;
        if attachment_id(&parent, &listed, generation, Some(&text)) != attachment
            || !state.same_publication(&current)
            || generation != self.attachment_generation(&current, &parent.object.id().library)?
        {
            return Err(error(ErrorCode::NotFound));
        }
        match text {
            TextState::Text(text) => {
                cancel.check()?;
                Ok((state, parent, text))
            }
            TextState::NotIndexed | TextState::NoFile | TextState::NotYet | TextState::Refused(_) => Err(
                EngineError::because(ErrorCode::SourceUnavailable, ErrorReason::NoAttachmentText),
            ),
        }
    }
}

/// Only prove membership: exact labels or fully written decimal ranges.
/// Abbreviated, Roman or prefixed ranges stay unknown pending confirmation.
fn label_in_reference_pages(label: &str, pages: &Value) -> bool {
    if let Some(page) = pages.as_u64() {
        return label.parse::<u64>() == Ok(page);
    }
    let Some(pages) = pages.as_str().filter(|pages| pages.len() <= 1024) else {
        return false;
    };
    pages.split(',').any(|range| {
        let range = range.trim();
        if range == label {
            return true;
        }
        let range = range.replace("--", "-");
        let Some((first, last)) = range.split_once(['-', '–', '—']) else {
            return false;
        };
        match (
            first.trim().parse::<u64>(),
            last.trim().parse::<u64>(),
            label.parse::<u64>(),
        ) {
            (Ok(first), Ok(last), Ok(page)) => (first..=last).contains(&page),
            _ => false,
        }
    })
}

/// The text of a library file's PDF attachment, found from the file's folder
/// while the file still holds the reference as the library read it.
fn file_text(
    reader: &PdfReader,
    library: &std::path::Path,
    parent: &SnapshotObject,
    attachment: &SourceAttachment,
    outside_library: bool,
    cancel: &Cancellation,
) -> Result<TextState, EngineError> {
    let file = ibis_sources::attachment_file(
        library,
        parent.object.id(),
        &parent.fingerprints.full,
        &attachment.key,
        outside_library,
        &|| cancel.is_cancelled(),
    )
    .map_err(|failure| match failure {
        // The reference changed since the library was read: the id is stale.
        ibis_sources::SourceError::Changing => error(ErrorCode::NotFound),
        failure => sources::source_error(failure),
    })?;
    cancel.check()?;
    let cancelled = cancel.clone();
    let text = match file {
        Some(file) => {
            ibis_sources::reading::text_of(None, Some((reader, &file)), Arc::new(move || cancelled.is_cancelled()))
        }
        None => TextState::Refused(Refusal::Missing),
    };
    cancel.check()?;
    Ok(text)
}

/// The health of the one library an attachment came from.
fn own_health(state: &State, parent: &SnapshotObject) -> health::SourceHealth {
    consulted(
        state,
        &ItemSelector::Id(parent.object.id().clone()),
        &parent.object.id().library,
    )
}

/// One check selects the current attachment metadata and library generation;
/// another binds the indexed text. Reading verifies both before publishing.
fn attachment_id(
    parent: &SnapshotObject,
    attachment: &SourceAttachment,
    generation: u64,
    text: Option<&TextState>,
) -> String {
    let reference = parent.object.id().opaque_reference();
    let slot = attachment_slot(parent, attachment, generation);
    let check = match text {
        Some(TextState::Text(text)) => text.revision.clone(),
        None => Fingerprint::of_bytes(b"too-large"),
        Some(TextState::NoFile) => Fingerprint::of_bytes(b"no-file"),
        Some(TextState::NotIndexed) => Fingerprint::of_bytes(b"not-indexed"),
        Some(TextState::NotYet) => Fingerprint::of_bytes(b"not-yet"),
        Some(TextState::Refused(refusal)) => Fingerprint::of_bytes(format!("refused:{refusal:?}").as_bytes()),
    };
    format!(
        "{ID_MARK}{}.{slot}.{}",
        reference.strip_prefix("id:").unwrap_or(&reference),
        &check.as_str()[..SLOT_LENGTH]
    )
}

fn attachment_slot(parent: &SnapshotObject, attachment: &SourceAttachment, generation: u64) -> String {
    let slot = Fingerprint::of_bytes(
        format!(
            "ibis attachment\0{}\0{generation}\0{:?}\0{}\0{:?}\0{:?}\0{}",
            parent.fingerprints.full.as_str(),
            attachment.key,
            attachment.revision.as_str(),
            attachment.title,
            attachment.media_type,
            attachment.link_only
        )
        .as_bytes(),
    );
    slot.as_str()[..SLOT_LENGTH].into()
}

/// The reference an id names, and its check, or not found.
fn parse_id<'a>(state: &State, id: &'a str) -> Result<(SnapshotObject, &'a str), EngineError> {
    let not_found = || error(ErrorCode::NotFound);
    let (reference, slot) = id
        .strip_prefix(ID_MARK)
        .and_then(|rest| rest.split_once('.'))
        .filter(|(_, check)| {
            check.split_once('.').is_some_and(|(slot, text)| {
                [slot, text]
                    .iter()
                    .all(|part| part.len() == SLOT_LENGTH && part.bytes().all(|byte| byte.is_ascii_hexdigit()))
            })
        })
        .ok_or_else(not_found)?;
    if reference.len() > 128 {
        return Err(not_found());
    }
    let snapshot = state.index.snapshot();
    let parent = snapshot
        .resolve_opaque(&format!("id:{reference}"))
        .and_then(|item| snapshot.get(item))
        .ok_or_else(not_found)?;
    Ok((parent.clone(), slot))
}

/// Text an agent or a terminal may be shown: no hidden or control characters
/// but line breaks and tabs.
fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .map(|character| match character {
            '\r' => '\n',
            '\u{c}' => ' ',
            other => other,
        })
        .filter(|character| !ibis_core::model::is_hidden(*character))
        .collect()
}

/// The largest char boundary of `text` at or before `at`.
fn floor(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn no_such_pages() -> EngineError {
    EngineError::because(ErrorCode::BadInput, ErrorReason::NoSuchPages)
}

/// The pages a read asks for, within both bounds, and where the next read starts.
fn read_pages(text: &DocumentText, request: &ReadRequest) -> Result<(Vec<PageText>, Option<ReadNext>), EngineError> {
    let offset = request.offset.unwrap_or(0);
    let pages = match &text.body {
        Body::Whole(whole) => {
            if request.first_page.is_some() || request.last_page.is_some() || offset > whole.len() {
                return Err(no_such_pages());
            }
            let start = floor(whole, offset);
            let end = floor(whole, start.saturating_add(MAX_READ_BYTES));
            let page = PageText {
                pdf_page: None,
                cite_page: None,
                text: clean(&whole[start..end]),
                cut: false,
            };
            let next = (end < whole.len()).then_some(ReadNext {
                first_page: None,
                offset: Some(end),
            });
            return Ok((vec![page], next));
        }
        Body::Pages(pages) => pages,
    };
    let count = u32::try_from(pages.len()).map_err(|_| error(ErrorCode::ResourceLimit))?;
    let first = request.first_page.unwrap_or(1);
    let last = request
        .last_page
        .unwrap_or(u32::MAX)
        .min(first.saturating_add(MAX_READ_PAGES - 1))
        .min(count);
    if first == 0 || first > count || request.last_page.is_some_and(|asked| asked < first) {
        return Err(no_such_pages());
    }
    let mut read = Vec::new();
    let mut left = MAX_READ_BYTES;
    let mut next = None;
    for number in first..=last {
        let page = &pages[(number - 1) as usize];
        let start = if number == first {
            if offset > page.len() {
                return Err(no_such_pages());
            }
            floor(page, offset)
        } else {
            0
        };
        let rest = &page[start..];
        if rest.len() <= left {
            left -= rest.len();
            read.push(PageText {
                pdf_page: Some(number),
                cite_page: text.cite_page(number),
                text: clean(rest),
                cut: false,
            });
            continue;
        }
        if read.is_empty() {
            // A page longer than one read holds: its start, then the rest.
            let end = start + floor(rest, left);
            read.push(PageText {
                pdf_page: Some(number),
                cite_page: text.cite_page(number),
                text: clean(&page[start..end]),
                cut: true,
            });
            next = Some(ReadNext {
                first_page: Some(number),
                offset: Some(end),
            });
        } else {
            next = Some(ReadNext {
                first_page: Some(number),
                offset: None,
            });
        }
        break;
    }
    if next.is_none() && last < count {
        next = Some(ReadNext {
            first_page: Some(last + 1),
            offset: None,
        });
    }
    Ok((read, next))
}

/// The first passages of `text` that hold `query`, already folded, and
/// whether they are all there are.
fn find_passages(text: &DocumentText, query: &str, cancel: &Cancellation) -> Result<(Vec<Passage>, bool), EngineError> {
    let pages: Vec<(Option<u32>, &str)> = match &text.body {
        Body::Pages(pages) => pages
            .iter()
            .enumerate()
            .map(|(index, page)| (u32::try_from(index + 1).ok(), page.as_str()))
            .collect(),
        Body::Whole(whole) => vec![(None, whole.as_str())],
    };
    let mut passages = Vec::new();
    for (page, body) in pages {
        cancel.check()?;
        for (start, end) in matches(body, query) {
            if passages.len() == MAX_PASSAGES {
                return Ok((passages, false));
            }
            passages.push(Passage {
                pdf_page: page,
                cite_page: page.and_then(|page| text.cite_page(page)),
                text: passage(body, start, end),
            });
        }
    }
    Ok((passages, true))
}

/// Compatibility-normalized lowercase text with each run of white space one
/// space, and for each byte of that the byte of `text` it came from.
fn fold(text: &str) -> (String, Vec<u32>) {
    let mut folded = String::with_capacity(text.len());
    let mut origins = Vec::with_capacity(text.len());
    let mut space = false;
    for (at, character) in text.char_indices() {
        let at = u32::try_from(at).unwrap_or(u32::MAX);
        if character.is_whitespace() || ibis_core::model::is_hidden(character) {
            if !space && character.is_whitespace() {
                folded.push(' ');
                origins.push(at);
                space = true;
            }
            continue;
        }
        space = false;
        for lower in std::iter::once(character).nfkc().flat_map(char::to_lowercase) {
            let before = folded.len();
            folded.push(lower);
            origins.extend(std::iter::repeat_n(at, folded.len() - before));
        }
    }
    (folded, origins)
}

/// Where `query`, already folded, appears in `text`, without overlapping
/// passages: byte ranges of `text`.
fn matches(text: &str, query: &str) -> Vec<(usize, usize)> {
    let (folded, origins) = fold(text);
    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut covered = 0;
    for (at, _) in folded.match_indices(query) {
        let start = origins[at] as usize;
        let last = origins[at + query.len() - 1] as usize;
        let end = last + text[last..].chars().next().map_or(0, char::len_utf8);
        if start < covered {
            continue;
        }
        covered = passage_bounds(text, start, end).1;
        found.push((start, end));
        if found.len() > MAX_PASSAGES {
            break;
        }
    }
    found
}

/// About [`PASSAGE_CHARS`] characters of `text` around `start..end`.
fn passage_bounds(text: &str, start: usize, end: usize) -> (usize, usize) {
    let length = text[start..end].chars().count();
    let before = PASSAGE_CHARS.saturating_sub(length) / 2;
    let from = text[..start]
        .char_indices()
        .rev()
        .nth(before.saturating_sub(1))
        .map_or(0, |(at, _)| at);
    let from = if before == 0 { start } else { from };
    let to = text[from..]
        .char_indices()
        .nth(PASSAGE_CHARS)
        .map_or(text.len(), |(at, _)| from + at);
    (from, to.max(end))
}

fn passage(text: &str, start: usize, end: usize) -> String {
    let (from, to) = passage_bounds(text, start, end);
    let words = clean(&text[from..to]).split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "{}{words}{}",
        if from > 0 { "…" } else { "" },
        if to < text.len() { "…" } else { "" }
    )
}

#[cfg(test)]
#[path = "reading_tests.rs"]
mod tests;
