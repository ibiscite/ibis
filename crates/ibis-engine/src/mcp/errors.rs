//! Supported tool validation outcomes: words and arguments stay in the catalogue.
use ibis_core::contract::{ErrorCode, Problem, Recovery as R, RecoveryInfo};

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ToolProblem {
    Arguments,
    UnknownArgument,
    Query,
    Limit,
    Shorthand,
    Reference,
    StaleReference,
    Items,
    TooManyItems,
    App,
    KeepOrder,
    Strict,
    OutputChoice,
    Format,
    CitationFormat,
    Style,
    Locale,
    KeyLocale,
    Form,
    HiddenCitation,
    RecordsOrItems,
    RecordsOrder,
    Records,
    Item,
    Text,
    Locator,
    OmitAuthor,
    EmptyOutput,
    NumberRange,
    NumberMinimum,
    StyleQuery,
    NoLibrary,
}
impl ToolProblem {
    pub(super) fn problem(self, args: &[(&str, &str)]) -> (ErrorCode, Problem) {
        let (code, recovery) = match self {
            Self::NoLibrary => (ErrorCode::SourceUnavailable, R::ChooseLibrary),
            Self::StaleReference => (ErrorCode::NotFound, R::SearchReferences),
            Self::Query | Self::StyleQuery => (ErrorCode::BadInput, R::EditInput),
            Self::TooManyItems => (ErrorCode::BadInput, R::ReduceSelection),
            Self::CitationFormat
            | Self::Format
            | Self::Style
            | Self::OutputChoice
            | Self::KeyLocale
            | Self::EmptyOutput => (ErrorCode::BadInput, R::ChooseStyle),
            Self::Locale => (ErrorCode::BadInput, R::ChooseLocale),
            Self::Locator => (ErrorCode::BadInput, R::EditLocator),
            Self::Arguments
            | Self::UnknownArgument
            | Self::Limit
            | Self::Shorthand
            | Self::Reference
            | Self::Items
            | Self::App
            | Self::KeepOrder
            | Self::Strict
            | Self::Form
            | Self::HiddenCitation
            | Self::RecordsOrItems
            | Self::RecordsOrder
            | Self::Records
            | Self::Item
            | Self::Text
            | Self::OmitAuthor
            | Self::NumberRange
            | Self::NumberMinimum => (ErrorCode::BadInput, R::EditInput),
        };
        let name = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        let mut info = RecoveryInfo::new(&format!("mcp-error-{name}"), recovery);
        for (name, value) in args {
            info = info.argument(name, *value);
        }
        (
            code,
            Problem {
                code: format!("invalid-{name}"),
                info,
            },
        )
    }
}
