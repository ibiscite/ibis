//! One bounded CSL formatter shared by the command line and desktop clients.
//! The pinned Zotero processor has no filesystem, network or native-input API.

mod catalog;
mod info;
mod input;
mod locale;
mod output;
mod runtime;
mod service;
mod yours;

pub use catalog::{
    Curated, Kept, LocaleEntry, Person, Release, Removed, StyleCredits, StyleEntry, StylePage, curated, files,
    keep_style, remove_style, style_credits, styles,
};
use ibis_core::contract::{
    CitationForm, CitationRequest, CitationResult, ItemRef, LibraryId, LibraryObject, OutputSpec, Record, Snapshot,
};
pub use service::{CancellationToken, FormatTask};
pub use yours::use_styles_folder;

/// The ids of your styles, sorted.
pub fn your_style_ids() -> Vec<String> {
    yours::ids().unwrap_or_default()
}
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Fixed diagnostics never include reference content or JavaScript exceptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FormatError {
    #[error("invalid citation request")]
    InvalidRequest,
    #[error("unsupported citation style")]
    UnsupportedStyle,
    #[error("unsupported output locale")]
    InvalidLocale,
    #[error("formatting cancelled")]
    Cancelled,
    #[error("formatting time limit exceeded")]
    TimedOut,
    #[error("formatting resource limit exceeded")]
    ResourceLimit,
    #[error("formatter queue is full")]
    Busy,
    #[error("formatter unavailable")]
    Runtime,
    /// A style of yours, or one being added, is not a usable CSL style.
    #[error("invalid citation style")]
    InvalidStyle,
    /// Your styles folder is not known or cannot be written.
    #[error("styles folder unavailable")]
    StylesFolder,
    /// You already keep a different version of this style.
    #[error("a different version is kept")]
    KeptDifferent,
}

// The native stack is four times the maximum QuickJS software budget.
pub(crate) const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;
const MAX_JS_STACK_BYTES: usize = WORKER_STACK_BYTES / 4;

/// Bounds are independent of library size. Zero never means unlimited.
#[derive(Debug, Clone)]
pub struct Limits {
    pub memory_bytes: usize,
    pub stack_bytes: usize,
    pub input_bytes: usize,
    pub output_bytes: usize,
    /// Maximum outstanding requests, including the running request.
    pub queue: usize,
    pub timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: 128 * 1024 * 1024,
            stack_bytes: MAX_JS_STACK_BYTES,
            input_bytes: 4 * 1024 * 1024,
            output_bytes: 4 * 1024 * 1024,
            queue: 8,
            timeout: Duration::from_secs(30),
        }
    }
}
impl Limits {
    fn valid(&self) -> bool {
        (1024 * 1024..=256 * 1024 * 1024).contains(&self.memory_bytes)
            && (16 * 1024..=MAX_JS_STACK_BYTES).contains(&self.stack_bytes)
            && (1..=16 * 1024 * 1024).contains(&self.input_bytes)
            && (1..=16 * 1024 * 1024).contains(&self.output_bytes)
            && (1..=64).contains(&self.queue)
            && !self.timeout.is_zero()
            && self.timeout <= Duration::from_secs(60)
    }
}

/// The styles checked most closely, each against a pinned Zotero result. All
/// the others come from the bundle: see [`styles`]. No runtime downloads.
pub const STYLES: [(&str, &str); 5] = [
    ("apa", "APA (7th edition)"),
    ("chicago-author-date", "Chicago (author-date)"),
    ("harvard-cite-them-right", "Harvard (Cite Them Right)"),
    ("ieee", "IEEE"),
    ("modern-language-association", "MLA (9th edition)"),
];

/// The SHA-256 of each file in [`STYLES`] whose author-only, year-only and
/// narrative output was checked against Zotero. The forms follow the file's
/// contents, so a kept copy of a checked version keeps them; a new upstream
/// version has them once it is checked again and its hash is added here.
pub(crate) const VERIFIED: [&str; 5] = [
    "3917def5fc951b104d64216c3373096042368b9704ba269ca83c14ebaba889fc",
    "002fade78d7e4fe9d42936a16b43a8066b097013f6255df40b1bfba6631eff9b",
    "0ca3d2c7c881cd98cdd81098b5349f5921edf430498aa9a5a41918621cf7cc45",
    "b4c7619fc16c45a31e4cc3271eab94ffe83192d3b4c7fc729470a3b459448de3",
    "e1432698ef28938d044b06bd45ddcc57d6b974cec25f71007075646a11f99484",
];

/// The forms one reference can be cited in with the style `id`.
pub fn style_forms(id: &str) -> Result<Vec<CitationForm>, FormatError> {
    input::style_forms(id)
}

/// Whether the style `id` writes reference entries, never in-text citations.
pub fn style_writes_entries(id: &str) -> Result<bool, FormatError> {
    input::style_writes_entries(id)
}

/// What adding a style did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct Added {
    /// The id it is kept under, from its own `<id>`.
    pub id: String,
    /// It replaced a style of yours with that id.
    pub replaced: bool,
}

#[derive(Clone)]
pub struct Formatter {
    service: Arc<service::Service>,
}
impl Formatter {
    pub fn new() -> Result<Self, FormatError> {
        Self::with_limits(Limits::default())
    }
    pub fn with_limits(limits: Limits) -> Result<Self, FormatError> {
        if !limits.valid() {
            return Err(FormatError::InvalidRequest);
        }
        Ok(Self {
            service: Arc::new(service::Service::new(limits)?),
        })
    }
    /// Requests share one bounded FIFO worker.
    pub fn format(&self, snapshot: &Snapshot, request: &CitationRequest) -> Result<CitationResult, FormatError> {
        self.service
            .submit(input::prepare(snapshot, request, self.service.limits())?)?
            .wait()
    }
    /// Refuse an unknown style or locale before formatting starts.
    pub fn check_output(output: &OutputSpec, locale: Option<&str>) -> Result<(), FormatError> {
        input::check_output(output, locale)
    }
    /// Add any `.csl` file to your styles, replacing yours of the same id. It
    /// is checked as it would be used, and must write a citation of an
    /// invented reference within the usual limits, before it is saved.
    pub fn add_style(&self, xml: &str) -> Result<Added, FormatError> {
        let (id, style) = catalog::candidate(xml)?;
        let item = ItemRef {
            library: LibraryId::digest(b"ibis-style-check"),
            key: "check".into(),
        };
        let csl = serde_json::from_value(serde_json::json!({
            "type": "book", "title": "Invented title", "publisher": "Invented Press",
            "author": [{"family": "Example", "given": "Avery"}], "issued": {"date-parts": [[2026]]}
        }))
        .map_err(|_| FormatError::Runtime)?;
        let snapshot = Snapshot::new(vec![LibraryObject::Record(Record {
            added_at: None,
            id: item.clone(),
            csl,
            citation_keys: vec![],
            aliases: vec![],
            warnings: vec![],
            identifiers: vec![],
        })])
        .map_err(|_| FormatError::Runtime)?;
        let request: CitationRequest = serde_json::from_value(serde_json::json!({
            "items": [{"item": item}], "output": {"kind": "csl", "style": id}
        }))
        .map_err(|_| FormatError::Runtime)?;
        let prepared = input::prepare_with_style(&snapshot, &request, self.service.limits(), &style)?;
        let result = self.service.submit(prepared)?.wait().map_err(|error| match error {
            FormatError::Runtime => FormatError::InvalidStyle,
            other => other,
        })?;
        if result
            .copy_content(1)
            .is_none_or(|content| content.text.trim().is_empty())
        {
            return Err(FormatError::InvalidStyle);
        }
        let replaced = yours::read(&id).is_ok_and(|existing| existing.is_some());
        yours::write(&id, xml)?;
        Ok(Added { id, replaced })
    }
    pub fn submit(&self, snapshot: Arc<Snapshot>, request: CitationRequest) -> Result<FormatTask, FormatError> {
        self.service
            .submit(input::prepare(&snapshot, &request, self.service.limits())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn software_stack_budget_has_a_measured_margin_and_a_hard_cap() {
        let maximum = Limits {
            stack_bytes: 2 * 1024 * 1024,
            ..Limits::default()
        };
        assert!(maximum.valid());
        assert!(maximum.stack_bytes * 4 <= WORKER_STACK_BYTES);
        assert_eq!(Limits::default().stack_bytes, maximum.stack_bytes);
        assert!(
            !Limits {
                stack_bytes: maximum.stack_bytes + 1,
                ..maximum
            }
            .valid()
        );
    }
}
