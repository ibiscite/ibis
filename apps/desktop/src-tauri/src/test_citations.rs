//! Native tests build engine requests and exercise the actual native clipboard boundary.
use crate::library::CommandError;
use ibis_core::contract::{CitedWarning, ClipboardContent, Draft, DraftItem, OutputSpec, Preview};
use ibis_engine::{Cancellation, Engine, citations::SessionId};
/// Warnings in the app's words.
pub(crate) fn said(warnings: &[CitedWarning]) -> Vec<String> {
    warnings
        .iter()
        .map(|warning| ibis_engine::catalogue::message(&warning.message, ibis_engine::catalogue::Audience::App))
        .collect()
}
pub(crate) fn preview(
    engine: &Engine,
    sample: bool,
    items: Vec<DraftItem>,
    output: &str,
) -> Result<Preview, CommandError> {
    let draft = Draft {
        items,
        output: OutputSpec::Preset { name: output.into() },
        form: Default::default(),
        keep_order: false,
        input_language: None,
        output_locale: None,
        sample,
    };
    Ok(engine
        .citations()
        .preview(SessionId::PICKER, draft, Cancellation::default())?
        .run()?)
}
pub(crate) fn copy(
    engine: &Engine,
    id: &str,
    hash: &str,
    write: impl FnOnce(ClipboardContent<'_>) -> Result<(), CommandError>,
) -> Result<crate::copy::Copied, CommandError> {
    crate::copy::copy_checked(engine, &engine.citations().commit(SessionId::PICKER, id, hash)?, write)
}
