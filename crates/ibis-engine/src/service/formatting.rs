//! Formatting and cancellation stay behind the same typed engine boundary.
use super::{Engine, StyleInfo, StylesResult, error};
use ibis_core::contract::*;
use ibis_csl::{CancellationToken, FormatError, Priority};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<CancelState>);
#[derive(Default)]
struct CancelState {
    cancelled: AtomicBool,
    handlers: Mutex<Handlers>,
}
#[derive(Default)]
struct Handlers {
    next: u64,
    tasks: BTreeMap<u64, CancellationToken>,
}
struct Registration {
    cancel: Cancellation,
    id: u64,
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.cancel
            .0
            .handlers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .tasks
            .remove(&self.id);
    }
}
impl Cancellation {
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        let tasks: Vec<_> = self
            .0
            .handlers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .tasks
            .values()
            .cloned()
            .collect();
        // Never hold the registration lock while entering the formatter scheduler.
        for task in tasks {
            task.cancel();
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<(), EngineError> {
        if self.is_cancelled() {
            Err(error(ErrorCode::Cancelled))
        } else {
            Ok(())
        }
    }
    fn track(&self, token: CancellationToken) -> Result<Registration, EngineError> {
        let mut handlers = self.0.handlers.lock().map_err(|_| error(ErrorCode::Internal))?;
        if handlers.tasks.len() >= 64 {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let id = handlers.next;
        handlers.next = id.checked_add(1).ok_or_else(|| error(ErrorCode::ResourceLimit))?;
        handlers.tasks.insert(id, token.clone());
        drop(handlers);
        // Covers cancellation before registration and races with cancel's snapshot.
        if self.is_cancelled() {
            token.cancel();
        }
        Ok(Registration {
            cancel: self.clone(),
            id,
        })
    }
}
impl Engine {
    pub fn styles(&self) -> Result<StylesResult, EngineError> {
        Ok(StylesResult {
            styles: ibis_csl::STYLES
                .into_iter()
                .map(|(id, title)| StyleInfo {
                    id: id.into(),
                    title: title.into(),
                })
                .collect(),
        })
    }
    pub(super) fn formatted(
        &self,
        snapshot: &Arc<Snapshot>,
        request: &CitationRequest,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        cancel.check()?;
        let formatter = {
            let mut current = self.shared.formatter.lock().map_err(|_| error(ErrorCode::Internal))?;
            if let Some(formatter) = current.as_ref() {
                formatter.clone()
            } else {
                // Construction only spawns workers. A transient OS resource failure
                // must not permanently disable formatting for this engine.
                let formatter = ibis_csl::Formatter::new().map_err(|_| error(ErrorCode::ResourceLimit))?;
                *current = Some(formatter.clone());
                formatter
            }
        };
        let priority = if matches!(request.form, CitationForm::BibliographyOnly | CitationForm::Full) {
            Priority::Background
        } else {
            Priority::Interactive
        };
        let task = match formatter.submit(snapshot.clone(), request.clone(), priority) {
            Ok(task) => task,
            Err(FormatError::UnsupportedStyle) => return Ok(refusal(request, CitationError::UnsupportedOutput)),
            Err(FormatError::InvalidLocale) => return Ok(refusal(request, CitationError::InvalidLocale)),
            Err(failure) => return Err(formatter_error(failure)),
        };
        let _registration = cancel.track(task.cancellation())?;
        let result = task.wait();
        cancel.check()?;
        result.map_err(formatter_error)
    }
}
fn formatter_error(failure: FormatError) -> EngineError {
    error(match failure {
        FormatError::Busy | FormatError::TimedOut | FormatError::ResourceLimit => ErrorCode::ResourceLimit,
        FormatError::Cancelled => ErrorCode::Cancelled,
        FormatError::InvalidRequest | FormatError::InvalidLocale | FormatError::UnsupportedStyle => ErrorCode::BadInput,
        FormatError::Runtime => ErrorCode::Internal,
    })
}
pub(super) fn refusal(request: &CitationRequest, failure: CitationError) -> CitationResult {
    CitationResult::new(
        String::new(),
        None,
        None,
        DisplayTree::default(),
        (0..request.items.len())
            .map(|item| ItemOutcome::Refused { item, error: failure })
            .collect(),
        vec![],
        vec![],
    )
}
