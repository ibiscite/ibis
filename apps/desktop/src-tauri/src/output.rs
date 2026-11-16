//! Presentation of the engine output catalogue.
pub(crate) use ibis_engine::app_context::output::{OutputRef, default_style, listed, stored};
use ibis_core::contract::OutputSpec;
use ibis_engine::outputs;
use serde::Serialize;

/// One output as the web view shows it, from the engine's list, written into
/// `bindings.ts` by `node scripts/gen.mjs`.
#[cfg(test)]
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct OutputChoice {
    id: String,
    /// The one name, in the header, footer, menus and Settings.
    name: String,
    /// A style's full title: the quiet detail beside the name.
    description: Option<String>,
    example: Option<String>,
    /// A citation style, written by the formatter, rather than a key syntax.
    style: bool,
    /// It writes reference entries, never in-text citations.
    entries: bool,
    /// It shares references (BibTeX, DOI links, CSL-JSON) rather than citing
    /// them: a one-off output, never remembered as a format (decision 0051).
    shares: bool,
}

/// Every output the picker offers, in the engine's order.
#[cfg(test)]
pub(crate) fn choices() -> Vec<OutputChoice> {
    outputs::outputs()
        .iter()
        .map(|output| OutputChoice {
            id: output.id.clone(),
            name: output.name.clone(),
            description: output.description.clone(),
            example: output.example.clone(),
            style: matches!(output.output, OutputSpec::Csl { .. }),
            entries: output.entries,
            shares: output.exports_records,
        })
        .collect()
}

