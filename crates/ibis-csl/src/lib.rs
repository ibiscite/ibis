//! The formatter: Zotero's own citeproc-js running in QuickJS, with the bundled
//! CSL styles and locales, bounded in time and memory. The pinned processor
//! has no filesystem, network or native-input API.
//!
//! It sits beside the readers in the [layers](https://github.com/ibiscite/ibis/blob/main/docs/architecture.md#layers)
//! and uses only `ibis-core`: it formats a core snapshot, wherever it came
//! from. The Rust API may change before 1.0.
//!
//! An APA citation of a one-record library:
//!
//! ```
//! use ibis_core::contract::{CitationRequest, ItemRef, LibraryId, LibraryObject, Record, Snapshot};
//! use serde_json::json;
//!
//! let id = ItemRef { library: LibraryId::digest(b"example library"), key: "work-1".into() };
//! let csl = json!({"type": "book", "title": "Synthetic signals", "issued": {"date-parts": [[2026]]},
//!     "author": [{"family": "Taylor", "given": "Avery"}]});
//! let record = Record {
//!     id: id.clone(),
//!     csl: serde_json::from_value(csl)?,
//!     citation_keys: vec![],
//!     aliases: vec![],
//!     warnings: vec![],
//!     identifiers: vec![],
//!     added_at: None,
//! };
//! let snapshot = Snapshot::new(vec![LibraryObject::Record(record)]).map_err(|error| format!("{error:?}"))?;
//! let request: CitationRequest = serde_json::from_value(json!({
//!     "items": [{"item": id}],
//!     "output": {"kind": "csl", "style": "apa"},
//! }))?;
//! let formatter = ibis_csl::Formatter::new()?;
//! let formatted = formatter.format(&snapshot, &request)?;
//! // Insert a citation only when every item rendered; `text` alone may be partial.
//! let content = formatted.copy_content(request.items.len()).ok_or("not every item rendered")?;
//! assert_eq!(content.text, "(Taylor, 2026)");
//!
//! // A reference the library does not hold is refused, not guessed.
//! let mut missing = request.clone();
//! missing.items[0].item.key = "not-in-library".into();
//! let refused = formatter.format(&snapshot, &missing)?;
//! assert!(refused.copy_content(missing.items.len()).is_none());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod catalog;
#[cfg(test)]
mod conformance;
mod input;
mod locale;
mod output;
mod runtime;
mod service;

pub use catalog::{Curated, LocaleEntry, Release, StyleEntry, StylePage, curated, files, styles};
use ibis_core::citable::FieldProjection;
use ibis_core::contract::{CitationForm, CitationRequest, CitationResult, OutputSpec, Snapshot};
pub use service::{CancellationToken, FormatTask};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Identify the exact shipped formatting material, without unpacking the catalogue.
pub fn provenance() -> ibis_core::contract::FormattingProvenance {
    use ibis_core::contract::{Fingerprint, FormattingProvenance};
    static PROVENANCE: std::sync::OnceLock<FormattingProvenance> = std::sync::OnceLock::new();
    PROVENANCE
        .get_or_init(|| FormattingProvenance {
            processor: Fingerprint::of_bytes(include_bytes!("../vendor/citeproc.mjs")),
            bundle: Fingerprint::of_bytes(include_bytes!("../vendor/csl-bundle.bin")),
        })
        .clone()
}

/// Fixed diagnostics never include reference content or JavaScript exceptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FormatError {
    #[error("citation request refused")]
    Citation(ibis_core::contract::CitationError),
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

/// The forms one reference can be cited in with the style `id`.
pub fn style_forms(id: &str) -> Result<Vec<CitationForm>, FormatError> {
    input::style_forms(id)
}

/// Whether the style `id` writes reference entries, never in-text citations.
pub fn style_writes_entries(id: &str) -> Result<bool, FormatError> {
    input::style_writes_entries(id)
}

#[derive(Clone)]
pub struct Formatter {
    service: Arc<service::Service>,
}
pub use input::ResolvedOutput;
pub use output::DocumentAnswer;

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
    /// Format an engine request with validated output and locale, projecting the
    /// bibliographic metadata before it reaches the processor.
    pub fn submit_resolved(
        &self,
        snapshot: Arc<Snapshot>,
        request: CitationRequest,
        output: &ResolvedOutput,
        project_fields: FieldProjection,
    ) -> Result<FormatTask, FormatError> {
        self.service.submit(input::prepare_resolved(
            &snapshot,
            &request,
            self.service.limits(),
            output,
            project_fields,
        )?)
    }
    pub fn submit(&self, snapshot: Arc<Snapshot>, request: CitationRequest) -> Result<FormatTask, FormatError> {
        self.service
            .submit(input::prepare(&snapshot, &request, self.service.limits())?)
    }
    /// One document's citations as one sequence, with its reference list
    /// (`ibis format`). `clusters` hold the indices of `request.items` each
    /// citation cites, in document order; every item is in exactly one.
    /// Note styles are refused: their citations belong in footnotes.
    /// The returned task shares the citation scheduler's cancellation and limits.
    pub fn submit_document(
        &self,
        snapshot: &Snapshot,
        request: &CitationRequest,
        output: &ResolvedOutput,
        clusters: &[Vec<usize>],
    ) -> Result<FormatTask, FormatError> {
        let prepared = input::prepare_document(snapshot, request, self.service.limits(), output, clusters)?;
        self.service.submit(prepared)
    }
    /// Whether the style `id` writes its citations as notes.
    pub fn style_writes_notes(id: &str) -> Result<bool, FormatError> {
        input::style_writes_notes(id)
    }
    /// Best-effort startup work. Call off the UI thread; no library is read and
    /// no citation is published. A foreground request retires late warm-ups.
    pub fn warm(&self, style: &str, locale: Option<&str>) -> Result<(), FormatError> {
        if let Some(task) = self.service.warm(input::warm(style, locale)?)? {
            task.wait()?;
        }
        Ok(())
    }
}
