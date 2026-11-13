//! Supported tool validation outcomes: words and arguments stay in the catalogue.
use ibis_core::contract::{ErrorCode, FocusTarget as F, Problem, Recovery as R, RecoveryInfo};

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
        let (code, recovery, focus) = match self {
            Self::NoLibrary => (ErrorCode::SourceUnavailable, R::ChooseLibrary, F::LibraryPicker),
            Self::StaleReference => (ErrorCode::NotFound, R::SelectReferences, F::Search),
            Self::Query | Self::StyleQuery => (ErrorCode::BadInput, R::EditInput, F::Search),
            Self::TooManyItems => (ErrorCode::BadInput, R::ReduceSelection, F::Results),
            Self::Format | Self::Style | Self::OutputChoice | Self::KeyLocale | Self::EmptyOutput => {
                (ErrorCode::BadInput, R::ChooseStyle, F::FormatPicker)
            }
            Self::Locale => (ErrorCode::BadInput, R::ChooseLocale, F::Language),
            Self::Locator => (ErrorCode::BadInput, R::EditCitation, F::Locator),
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
            | Self::NumberMinimum => (ErrorCode::BadInput, R::EditInput, F::Request),
        };
        let name = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        let mut info = RecoveryInfo::new(&format!("mcp-error-{name}"), recovery, focus);
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
