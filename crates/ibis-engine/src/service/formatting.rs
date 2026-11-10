//! Formatting and cancellation stay behind the same typed engine boundary.
use super::{Engine, StylesResult, error};
use crate::request::StylesRequest;
use ibis_core::contract::*;
use ibis_csl::{CancellationToken, FormatError};
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
    parent: Option<Cancellation>,
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
    /// Native inspection can expire without cancelling its caller's request.
    pub(crate) fn child(&self) -> Self {
        Self(Arc::new(CancelState {
            parent: Some(self.clone()),
            ..CancelState::default()
        }))
    }
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
        self.0.cancelled.load(Ordering::Acquire) || self.0.parent.as_ref().is_some_and(Self::is_cancelled)
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
    pub fn styles(&self, request: &StylesRequest) -> Result<StylesResult, EngineError> {
        if !(1..=20_000).contains(&request.limit)
            || request.query.as_ref().is_some_and(|query| query.len() > 1024)
            || request
                .id
                .as_ref()
                .is_some_and(|id| id.len() > 1024 || request.query.is_some())
        {
            return Err(error(ErrorCode::BadInput));
        }
        // One style asks for an empty page, which still carries the releases and languages.
        let (query, limit) = match &request.id {
            Some(_) => ("", 0),
            None => (request.query.as_deref().unwrap_or(""), request.limit),
        };
        let page = ibis_csl::styles(query, limit, request.offset).map_err(formatter_error)?;
        let credits = request
            .id
            .as_deref()
            .map(ibis_csl::style_credits)
            .transpose()
            .map_err(formatter_error)?;
        let (styles, total) = match &credits {
            Some((style, _)) => (vec![style.clone()], 1),
            None => (page.styles, page.total),
        };
        Ok(StylesResult {
            styles,
            total,
            locales: page.locales,
            styles_release: page.styles_release,
            locales_release: page.locales_release,
            credits: credits.map(|(_, credits)| credits),
        })
    }
    /// Keep the bundled style `id` as it is now in your styles (decision 0007).
    pub fn keep_style(&self, id: &str) -> Result<ibis_csl::Kept, EngineError> {
        ibis_csl::keep_style(id).map_err(formatter_error)
    }
    /// Add a `.csl` file to your styles, after checking it as it would be used.
    pub fn add_style(&self, xml: &str) -> Result<ibis_csl::Added, EngineError> {
        self.formatter()?.add_style(xml).map_err(formatter_error)
    }
    /// Remove your style `id`; the bundled style of that id, if any, applies again.
    pub fn remove_style(&self, id: &str) -> Result<ibis_csl::Removed, EngineError> {
        ibis_csl::remove_style(id).map_err(formatter_error)
    }
    fn formatter(&self) -> Result<ibis_csl::Formatter, EngineError> {
        let mut current = self.shared.formatter.lock().map_err(|_| error(ErrorCode::Internal))?;
        if let Some(formatter) = current.as_ref() {
            return Ok(formatter.clone());
        }
        // Construction only spawns the worker. A transient OS resource failure
        // must not permanently disable formatting for this engine.
        let formatter = ibis_csl::Formatter::new().map_err(|_| error(ErrorCode::ResourceLimit))?;
        *current = Some(formatter.clone());
        Ok(formatter)
    }
    pub(super) fn formatted(
        &self,
        snapshot: &Arc<Snapshot>,
        request: &CitationRequest,
        cancel: &Cancellation,
    ) -> Result<CitationResult, EngineError> {
        cancel.check()?;
        // The request is wrong, not its items: refuse it whole, before formatting.
        ibis_csl::Formatter::check_output(&request.output, request.output_locale.as_deref())
            .map_err(formatter_error)?;
        let annotations: Vec<_> = request
            .items
            .iter()
            .map(|pick| super::prepared::annotation_error(snapshot, pick, None))
            .collect();
        if annotations.iter().any(Option::is_some) {
            return Ok(CitationResult::new(
                String::new(),
                None,
                None,
                DisplayTree::default(),
                annotations
                    .into_iter()
                    .enumerate()
                    .map(|(item, error)| ItemOutcome::Refused {
                        item,
                        error: error.unwrap_or(CitationError::UnsupportedForm),
                    })
                    .collect(),
                vec![],
                vec![],
            ));
        }
        let formatter = self.formatter()?;
        let task = match formatter.submit(snapshot.clone(), request.clone()) {
            Ok(task) => task,
            Err(failure) => return Err(formatter_error(failure)),
        };
        let _registration = cancel.track(task.cancellation())?;
        let result = task.wait();
        cancel.check()?;
        result.map_err(formatter_error)
    }
}
fn formatter_error(failure: FormatError) -> EngineError {
    match failure {
        FormatError::Busy | FormatError::TimedOut | FormatError::ResourceLimit => error(ErrorCode::ResourceLimit),
        FormatError::Cancelled => error(ErrorCode::Cancelled),
        FormatError::InvalidRequest => error(ErrorCode::BadInput),
        FormatError::UnsupportedStyle => EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownStyle),
        FormatError::InvalidLocale => EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownLocale),
        FormatError::Runtime => error(ErrorCode::Internal),
        FormatError::InvalidStyle => EngineError::because(ErrorCode::BadInput, ErrorReason::InvalidStyle),
        FormatError::StylesFolder => EngineError::because(ErrorCode::SourceUnavailable, ErrorReason::StylesFolder),
        FormatError::KeptDifferent => EngineError::because(ErrorCode::BadInput, ErrorReason::KeptDifferent),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::time::{Duration, Instant};
    fn case() -> GoldenCase {
        serde_json::from_str(include_str!("../../../../fixtures/cases/02-pandoc.json")).unwrap()
    }
    #[test]
    fn cancellation_reaches_format_workers_and_next_request_recovers() {
        let case = case();
        let LibraryObject::Record(template) = &case.library.objects()[0].object else {
            panic!("record")
        };
        let mut request = case.request.clone();
        request.output = OutputSpec::Csl { style: "apa".into() };
        request.form = CitationForm::BibliographyOnly;
        request.keep_order = false;
        request.items.clear();
        let objects = (0..1000)
            .map(|index| {
                let mut record = template.clone();
                record.id.key = format!("synthetic-{index}");
                record
                    .csl
                    .insert("title".into(), serde_json::json!(format!("Synthetic title {index}")));
                let mut pick = case.request.items[0].clone();
                pick.item = record.id.clone();
                pick.expected_cite = None;
                request.items.push(pick);
                LibraryObject::Record(record)
            })
            .collect();
        let engine = Engine::from_snapshot(Arc::new(Snapshot::new(objects).unwrap()));
        let cancel = Cancellation::default();
        std::thread::scope(|scope| {
            let working = scope.spawn(|| engine.cite(&request, &cancel));
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut observed = false;
            while Instant::now() < deadline && !working.is_finished() {
                observed = !cancel.0.handlers.lock().unwrap().tasks.is_empty();
                if observed {
                    break;
                }
                std::thread::yield_now();
            }
            cancel.cancel();
            assert_eq!(working.join().unwrap().unwrap_err().code, ErrorCode::Cancelled);
            assert!(observed, "format request was registered for cancellation");
        });
        assert!(cancel.0.handlers.lock().unwrap().tasks.is_empty());
        request.items.truncate(1);
        assert!(
            engine
                .cite(&request, &Cancellation::default())
                .unwrap()
                .copy_content(1)
                .is_some()
        );
    }
    #[test]
    fn oversized_csl_is_a_resource_refusal_and_worker_recovers() {
        let mut case = case();
        case.request.output = OutputSpec::Csl { style: "apa".into() };
        case.request.items[0].expected_cite = None;
        let LibraryObject::Record(mut record) = case.library.objects()[0].object.clone() else {
            panic!("record")
        };
        record
            .csl
            .insert("title".into(), serde_json::json!("x".repeat(4 * 1024 * 1024)));
        let engine = Engine::from_snapshot(Arc::new(Snapshot::new(vec![LibraryObject::Record(record)]).unwrap()));
        assert_eq!(
            engine.cite(&case.request, &Cancellation::default()).unwrap_err().code,
            ErrorCode::ResourceLimit
        );
        let id = case.request.items[0].item.library.clone();
        engine
            .publish_library(id, crate::LibraryKind::Snapshot, Arc::new(case.library))
            .unwrap();
        assert!(
            engine
                .cite(&case.request, &Cancellation::default())
                .unwrap()
                .copy_content(1)
                .is_some()
        );
    }
}
