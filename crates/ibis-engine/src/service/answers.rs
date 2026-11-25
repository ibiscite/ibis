//! The engine function behind each row of the method table (decision 0057):
//! written by hand, routed by the table.
use super::{Cancellation, DispatchResult, Engine, StylesResult, response_schema};
use crate::Method;
use crate::outputs::OutputsResult;
use crate::request::{OutputsRequest, SchemaRequest, StylesRequest, request_schema};
use ibis_core::contract::{EngineError, ExitCode};
use serde::Serialize;
use serde_json::{Value, json};

pub(crate) fn styles(
    engine: &Engine,
    request: StylesRequest,
    cancel: &Cancellation,
) -> Result<DispatchResult<StylesResult>, EngineError> {
    catalogue(engine.styles(&request)?, cancel)
}

pub(crate) fn outputs(
    _: &Engine,
    OutputsRequest {}: OutputsRequest,
    cancel: &Cancellation,
) -> Result<DispatchResult<OutputsResult>, EngineError> {
    catalogue(crate::outputs::list(), cancel)
}

pub(crate) fn schema(
    _: &Engine,
    request: SchemaRequest,
    cancel: &Cancellation,
) -> Result<DispatchResult<Value>, EngineError> {
    let methods = request
        .method
        .map_or_else(|| Method::ALL.to_vec(), |method| vec![method]);
    let schemas = methods
        .into_iter()
        .map(|method| {
            json!({
                "method": method,
                "contract": ibis_core::contract::CONTRACT_VERSION,
                "params": request_schema(method),
                "result": response_schema(method),
            })
        })
        .collect();
    catalogue(Value::Array(schemas), cancel)
}

/// Styles, outputs and schemas read no library, so no library's health bears on them.
fn catalogue<T: Serialize>(data: T, cancel: &Cancellation) -> Result<DispatchResult<T>, EngineError> {
    cancel.check()?;
    DispatchResult {
        data,
        exit_code: ExitCode::Ok,
        text: None,
        source_findings: false,
    }
    .bounded()
}
