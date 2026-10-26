//! Generated vocabulary for front ends; method schemas are added by the engine.
use super::{CitationRequest, CitationResult, Envelope, ExitCode, Snapshot};
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};

/// Shared fixture format, replayed by each front end as it adopts the contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GoldenCase {
    pub name: String,
    pub library: Snapshot,
    pub request: CitationRequest,
    pub expected: Envelope<CitationResult>,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct EngineContract {
    case: GoldenCase,
    exit_code: ExitCode,
}

/// The exact JSON Schema checked into `fixtures/engine-contract.schema.json`.
pub fn schema() -> Schema {
    schema_for!(EngineContract)
}
