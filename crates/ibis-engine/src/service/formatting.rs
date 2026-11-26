//! Formatting and cancellation stay behind the same typed engine boundary.
use super::{Engine, StylesResult, error};
use crate::request::StylesRequest;
use crate::sync::MutexExt;
use ibis_core::citable::FieldProjection;
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
pub(super) struct Registration {
    parents: Vec<Registration>,
    cancel: Cancellation,
    id: u64,
}

/// A lazy formatter owner, shareable by libraries in one desktop session.
/// Key citations leave its worker unstarted unless the client explicitly warms it.
#[derive(Clone, Default)]
pub struct Formatting(Arc<Mutex<Option<ibis_csl::Formatter>>>);
impl Formatting {
    pub(super) fn formatter(&self) -> Result<ibis_csl::Formatter, EngineError> {
        let mut current = self.0.recover_lock();
        if let Some(formatter) = current.as_ref() {
            return Ok(formatter.clone());
        }
        // Construction only spawns the worker. A transient OS resource failure
        // must not permanently disable formatting for this owner.
        let formatter = ibis_csl::Formatter::new().map_err(|_| error(ErrorCode::ResourceLimit))?;
        *current = Some(formatter.clone());
        Ok(formatter)
    }
    /// Call in the background at startup, independently of source acquisition.
    /// No library state, settings or citation receipts are changed.
    pub fn warm(&self, style: &str, locale: Option<&str>) -> Result<(), EngineError> {
        self.formatter()?.warm(style, locale).map_err(formatter_error)
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.cancel.0.handlers.recover_lock().tasks.remove(&self.id);
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
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        let tasks: Vec<_> = self.0.handlers.recover_lock().tasks.values().cloned().collect();
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
    pub(super) fn track(&self, token: CancellationToken) -> Result<Registration, EngineError> {
        let mut handlers = self.0.handlers.recover_lock();
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
        let mut registration = Registration {
            cancel: self.clone(),
            id,
            parents: vec![],
        };
        if let Some(parent) = &self.0.parent {
            registration.parents.push(parent.track(token)?);
        }
        Ok(registration)
    }
}
impl Engine {
    pub fn styles(&self, request: &StylesRequest) -> Result<StylesResult, EngineError> {
        if !(1..=20_000).contains(&request.limit) || request.query.as_ref().is_some_and(|query| query.len() > 1024) {
            return Err(error(ErrorCode::BadInput));
        }
        ibis_csl::styles(request.query.as_deref().unwrap_or(""), request.limit, request.offset).map_err(formatter_error)
    }
    pub(super) fn formatted(
        &self,
        snapshot: &Arc<Snapshot>,
        request: &CitationRequest,
        cancel: &Cancellation,
        output: &ibis_csl::ResolvedOutput,
        project_fields: FieldProjection,
    ) -> Result<CitationResult, EngineError> {
        cancel.check()?;
        let formatter = self.formatting.formatter()?;
        let task = formatter
            .submit_resolved(snapshot.clone(), request.clone(), output, project_fields)
            .map_err(formatter_error)?;
        let _registration = cancel.track(task.cancellation())?;
        let result = task.wait();
        cancel.check()?;
        result.map_err(formatter_error)
    }
}
pub(crate) fn formatter_error(failure: FormatError) -> EngineError {
    match failure {
        FormatError::Citation(error) => EngineError::citation(error),
        FormatError::Busy | FormatError::TimedOut | FormatError::ResourceLimit => error(ErrorCode::ResourceLimit),
        FormatError::Cancelled => error(ErrorCode::Cancelled),
        FormatError::InvalidRequest => error(ErrorCode::BadInput),
        FormatError::UnsupportedStyle => EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownStyle),
        FormatError::InvalidLocale => EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownLocale),
        FormatError::Runtime => error(ErrorCode::Internal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn case() -> GoldenCase {
        serde_json::from_str(include_str!("../../../../fixtures/cases/02-pandoc.json")).unwrap()
    }
    #[test]
    fn startup_formatting_does_not_wait_for_library_acquisition() {
        let formatting = Formatting::default();
        std::thread::scope(|scope| {
            let (release, held) = std::sync::mpsc::channel();
            let (finished, warmed) = std::sync::mpsc::channel();
            let loading = formatting.clone();
            let library = scope.spawn(move || {
                // The source cannot finish until formatting has warmed.
                held.recv_timeout(Duration::from_secs(5)).unwrap();
                Engine::from_snapshot(Arc::new(case().library)).with_formatting(loading)
            });
            scope.spawn(move || finished.send(formatting.warm("apa", None)).unwrap());
            let warm = warmed.recv_timeout(Duration::from_secs(5));
            release.send(()).unwrap();
            warm.unwrap().unwrap();
            let engine = library.join().unwrap();
            let mut request = case().request;
            request.output = OutputSpec::Csl { style: "apa".into() };
            let preview = engine.cite(&request, &Cancellation::default()).unwrap();
            assert!(!preview.text.is_empty());
            assert!(preview.copy_content(request.items.len()).is_some());
        });
    }

    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "Synchronizes cancellation with the formatter task registration."
    )]
    fn document_cancellation_releases_the_formatter_without_writing_a_copy() {
        use std::io::{Cursor, Write};
        use std::time::Instant;

        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().canonicalize().unwrap();
        let document = root.join("draft.docx");
        let output = root.join("final.docx");
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("[Content_Types].xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(
                b"<Types>application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml</Types>",
            )
            .unwrap();
        archive
            .start_file("word/document.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        let citations = "<w:p><w:r><w:t>[@vale2024]</w:t></w:r></w:p>".repeat(1000);
        archive
            .write_all(format!("<w:document><w:body>{citations}</w:body></w:document>").as_bytes())
            .unwrap();
        std::fs::write(&document, archive.finish().unwrap().into_inner()).unwrap();
        let library = root.join("refs.bib");
        std::fs::write(
            &library,
            "@book{vale2024, author={Vale, Avery}, title={Invented Lanterns}, year={2024}}",
        )
        .unwrap();
        let engine = Engine::new()
            .unwrap()
            .without_file_watches()
            .with_documents_in_process();
        engine.open_file(&library, &Cancellation::default()).unwrap();
        let cancel = Cancellation::default();
        let request = super::super::FormatRequest {
            document,
            output: output.clone(),
            style: "apa".into(),
            output_locale: None,
        };
        std::thread::scope(|scope| {
            let (send, answer) = std::sync::mpsc::channel();
            let (engine, request, task_cancel) = (&engine, &request, &cancel);
            scope.spawn(move || send.send(engine.format_document(request, task_cancel)).unwrap());
            let deadline = Instant::now() + Duration::from_secs(5);
            while cancel.0.handlers.recover_lock().tasks.is_empty() && Instant::now() < deadline {
                if let Ok(finished) = answer.try_recv() {
                    panic!("document formatting completed without cancellable CSL work: {finished:?}");
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            cancel.cancel();
            assert_eq!(
                answer.recv_timeout(Duration::from_secs(1)).unwrap(),
                Err(error(ErrorCode::Cancelled))
            );
        });
        assert!(!output.exists());
    }
}
