//! Every output Ibis writes, each with one id and one name in every front end:
//! the key syntaxes and commands of [`ibis_core::syntax::CAPABILITIES`], then
//! the common styles of the bundled catalogue. The command line's `--format`,
//! MCP's `format`, the app's format menu and its settings all read this list;
//! none keeps its own.
use ibis_core::contract::{CitationForm, EngineError, ErrorCode, ErrorReason, OutputSpec};
use ibis_core::syntax::CAPABILITIES;
use schemars::JsonSchema;
use serde::Serialize;
use std::sync::LazyLock;

/// What a citation is written in when nothing names an output.
pub const DEFAULT: &str = "pandoc";
/// The style for finished text when nothing names one: what an app that wants
/// formatted citations gets, and what an agent's citation is written in.
pub const DEFAULT_STYLE: &str = "apa";

/// One output, as the `outputs` method lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct OutputInfo {
    /// Stable: settings store it, `--format` and MCP's `format` take it, and
    /// `{"kind":"preset","name":ID}` names it in a request.
    pub id: String,
    /// The one name every front end shows, short enough for "APA · Word".
    pub name: String,
    /// A style's full title, shown only as the quiet detail beside the name;
    /// none when it is the name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What it writes for one invented reference: the key `key`, or for a
    /// style an article by A. García (2021).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    /// The output a request names for it.
    pub output: OutputSpec,
    /// The forms one reference can be cited in.
    pub forms: Vec<CitationForm>,
    /// It writes each reference's whole record, notes included, not a
    /// citation; agents are not offered it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exports_records: bool,
    /// It writes reference entries, never in-text citations: a numbered
    /// style cannot number one citation alone.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub entries: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OutputsResult {
    /// The key outputs first, then the common styles, most used first.
    pub outputs: Vec<OutputInfo>,
    /// The id used when nothing names an output.
    pub default: String,
    /// The id of the style used when finished text is wanted and nothing names a style.
    pub default_style: String,
}

/// What each common style writes for the invented article in
/// [`OutputInfo::example`]; checked against the formatter by
/// `tests/outputs.rs`. A style missing here has no example.
const STYLE_EXAMPLES: [(&str, &str); 13] = [
    ("apa", "(García, 2021)"),
    ("chicago-author-date", "(García 2021)"),
    (
        "chicago-notes-bibliography",
        "Ana García, “Synthetic Sleep Study,” Journal of Tests 4 (2021).",
    ),
    ("modern-language-association", "(García)"),
    ("harvard-cite-them-right", "(García, 2021)"),
    (
        "ieee",
        "A. García, “Synthetic sleep study,” Journal of Tests, vol. 4, 2021.",
    ),
    (
        "vancouver-nlm",
        "García A. Synthetic sleep study. Journal of Tests. 2021;4.",
    ),
    (
        "nature",
        "García, A. Synthetic sleep study. Journal of Tests 4, (2021).",
    ),
    (
        "american-medical-association",
        "García A. Synthetic sleep study. Journal of Tests. 2021;4.",
    ),
    (
        "american-chemical-society",
        "García, A. Synthetic Sleep Study. Journal of Tests 2021, 4.",
    ),
    (
        "china-national-standard-gb-t-7714-2025-numeric",
        "García A. Synthetic sleep study[J]. Journal of Tests，2021，4.",
    ),
    ("china-national-standard-gb-t-7714-2025-author-date", "（García，2021）"),
    (
        "china-national-standard-gb-t-7714-2025-note",
        "García A. Synthetic sleep study[J]. Journal of Tests，2021，4.",
    ),
];

static OUTPUTS: LazyLock<Vec<OutputInfo>> = LazyLock::new(|| {
    let keys = CAPABILITIES.iter().map(|caps| OutputInfo {
        id: caps.id.to_owned(),
        name: caps.name.to_owned(),
        description: None,
        example: caps.example.map(str::to_owned),
        output: caps.output(),
        forms: caps.citable_forms(),
        exports_records: caps.exports_records,
        entries: false,
    });
    // A style the bundle cannot describe is left out rather than listed
    // with forms it may not have; the key outputs never depend on it.
    let styles = ibis_csl::curated().into_iter().flatten().filter_map(|style| {
        let id = style.id();
        Some(OutputInfo {
            id: id.to_owned(),
            name: style.name().to_owned(),
            // A title that is the name again ("IEEE") adds nothing.
            description: (style.title() != style.name()).then(|| style.title().to_owned()),
            example: STYLE_EXAMPLES
                .iter()
                .find(|(listed, _)| *listed == id)
                .map(|(_, example)| (*example).to_owned()),
            output: OutputSpec::Csl { style: id.to_owned() },
            forms: ibis_csl::style_forms(id).ok()?,
            exports_records: false,
            entries: ibis_csl::style_writes_entries(id).ok()?,
        })
    });
    keys.chain(styles).collect()
});

/// Every output, the key outputs first.
pub fn outputs() -> &'static [OutputInfo] {
    &OUTPUTS
}

/// The list as the `outputs` method returns it.
pub fn list() -> OutputsResult {
    OutputsResult {
        outputs: outputs().to_vec(),
        default: DEFAULT.to_owned(),
        default_style: DEFAULT_STYLE.to_owned(),
    }
}

/// The output with this id.
pub fn find(id: &str) -> Option<&'static OutputInfo> {
    outputs().iter().find(|output| output.id == id)
}

/// The output a stored id names, or the default when it names none: an id
/// from another version of Ibis, or typed by hand, fails closed.
pub fn or_default(id: Option<&str>) -> &'static OutputInfo {
    id.and_then(find)
        .or_else(|| find(DEFAULT))
        // The key outputs come first and always hold the default.
        .unwrap_or(&outputs()[0])
}

/// The entry that writes `output`, so a front end can say its name. A style
/// outside the common ones has none; its id is its name.
pub fn named(output: &OutputSpec) -> Option<&'static OutputInfo> {
    let output = resolve(output).ok()?;
    let caps = ibis_core::syntax::capabilities(&output).ok();
    outputs().iter().find(|entry| match caps {
        Some(caps) => entry.id == caps.id,
        None => entry.output == output,
    })
}

/// The output a request means: a preset becomes its entry's own output, and
/// any other output is already one.
pub fn resolve(output: &OutputSpec) -> Result<OutputSpec, EngineError> {
    match output {
        OutputSpec::Preset { name } => find(name)
            .map(|entry| entry.output.clone())
            .ok_or_else(|| EngineError::because(ErrorCode::BadInput, ErrorReason::UnknownOutput)),
        output => Ok(output.clone()),
    }
}
