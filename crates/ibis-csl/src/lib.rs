//! One bounded CSL formatter shared by the command line and desktop clients.
//! The pinned Zotero processor has no filesystem, network or native-input API.

mod input;
mod locale;
mod output;
mod runtime;
mod service;

use ibis_core::contract::{CitationRequest, CitationResult, Snapshot};
pub use service::{CancellationToken, FormatTask};
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Interactive,
    Background,
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
    pub interactive_queue: usize,
    pub background_queue: usize,
    pub interactive_timeout: Duration,
    pub background_timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: 128 * 1024 * 1024,
            stack_bytes: MAX_JS_STACK_BYTES,
            input_bytes: 4 * 1024 * 1024,
            output_bytes: 4 * 1024 * 1024,
            interactive_queue: 8,
            background_queue: 2,
            interactive_timeout: Duration::from_secs(5),
            background_timeout: Duration::from_secs(30),
        }
    }
}
impl Limits {
    fn valid(&self) -> bool {
        (1024 * 1024..=256 * 1024 * 1024).contains(&self.memory_bytes)
            && (16 * 1024..=MAX_JS_STACK_BYTES).contains(&self.stack_bytes)
            && (1..=16 * 1024 * 1024).contains(&self.input_bytes)
            && (1..=16 * 1024 * 1024).contains(&self.output_bytes)
            && (1..=64).contains(&self.interactive_queue)
            && (1..=64).contains(&self.background_queue)
            && !self.interactive_timeout.is_zero()
            && self.interactive_timeout <= Duration::from_secs(60)
            && !self.background_timeout.is_zero()
            && self.background_timeout <= Duration::from_secs(60)
    }
}

/// Five offline styles and their bundled English locale. No runtime downloads.
pub const STYLES: [(&str, &str); 5] = [
    ("apa", "APA (7th edition)"),
    ("chicago-author-date", "Chicago (author-date)"),
    ("harvard-cite-them-right", "Harvard (Cite Them Right)"),
    ("ieee", "IEEE"),
    ("modern-language-association", "MLA (9th edition)"),
];

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
    /// Bibliography-only requests use the background worker; ordinary copies/previews go first.
    pub fn format(&self, snapshot: &Snapshot, request: &CitationRequest) -> Result<CitationResult, FormatError> {
        let priority = if request.form == ibis_core::contract::CitationForm::BibliographyOnly {
            Priority::Background
        } else {
            Priority::Interactive
        };
        self.service
            .submit(input::prepare(snapshot, request, self.service.limits())?, priority)?
            .wait()
    }
    pub fn submit(
        &self,
        snapshot: Arc<Snapshot>,
        request: CitationRequest,
        priority: Priority,
    ) -> Result<FormatTask, FormatError> {
        self.service
            .submit(input::prepare(&snapshot, &request, self.service.limits())?, priority)
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
