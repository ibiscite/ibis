//! The method registry shared by typed engine calls and headless front ends.
use crate::outputs::OutputsResult;
use crate::request::{OutputsRequest, SchemaRequest, StylesRequest};
use crate::service::{Cancellation, DispatchResult, Engine, Reply, StylesResult, answers, parameters};
use ibis_core::contract::{EngineError, ErrorCode};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Libraries,
    Search,
    Show,
    Cite,
    Records,
    Project,
    Entry,
    Outline,
    Read,
    #[serde(rename = "find-in")]
    FindIn,
    Share,
    Styles,
    Outputs,
    Schema,
    Call,
}
impl Method {
    pub const ALL: [Self; 15] = [
        Self::Libraries,
        Self::Search,
        Self::Show,
        Self::Cite,
        Self::Records,
        Self::Project,
        Self::Entry,
        Self::Outline,
        Self::Read,
        Self::FindIn,
        Self::Share,
        Self::Styles,
        Self::Outputs,
        Self::Schema,
        Self::Call,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Libraries => "libraries",
            Self::Search => "search",
            Self::Show => "show",
            Self::Cite => "cite",
            Self::Records => "records",
            Self::Project => "project",
            Self::Entry => "entry",
            Self::Outline => "outline",
            Self::Read => "read",
            Self::FindIn => "find-in",
            Self::Share => "share",
            Self::Styles => "styles",
            Self::Outputs => "outputs",
            Self::Schema => "schema",
            Self::Call => "call",
        }
    }
    pub fn parse(name: &str) -> Result<Self, ErrorCode> {
        Self::ALL
            .into_iter()
            .find(|method| method.as_str() == name)
            .ok_or(ErrorCode::Usage)
    }
}

/// One public method, declared once in [`rows!`] (decision 0057): its wire
/// name, typed parameters and result, one catalogue line, and the engine
/// function that answers it. Routing and schemas are derived from the row;
/// the handler stays hand-written.
pub trait Row {
    const METHOD: Method;
    /// The wire name.
    const NAME: &'static str;
    /// One catalogue line.
    const DOCS: &'static str;
    type Params: DeserializeOwned + JsonSchema;
    type Result: Serialize + JsonSchema;
    fn answer(
        engine: &Engine,
        params: Self::Params,
        cancel: &Cancellation,
    ) -> Result<DispatchResult<Self::Result>, EngineError>;
    /// The result as native callers hold it beside other methods' results.
    fn reply(result: Self::Result) -> Reply;
}

/// What dispatch and the schemas read from a row, without its types.
#[derive(Clone, Copy)]
pub(crate) struct Entry {
    pub params: fn() -> Value,
    pub result: fn() -> Value,
    pub route: fn(&Engine, &Value, &Cancellation) -> Result<DispatchResult<Reply>, EngineError>,
}
impl Entry {
    const fn of<M: Row>() -> Self {
        Self {
            params: || schema_for!(M::Params).into(),
            result: || schema_for!(M::Result).into(),
            route: |engine, params, cancel| Ok(engine.call::<M>(parameters(params)?, cancel)?.map_data(M::reply)),
        }
    }
}

/// Each row: docs, `Type "wire-name": Params => Result, handler, reply`.
macro_rules! rows {
    ($(
        $(#[doc = $doc:literal])+
        $variant:ident $name:literal: $params:ty => $result:ty, $handler:path, $reply:expr;
    )*) => {
        /// One type per method, for typed calls: `engine.call::<methods::Styles>(…)`.
        pub mod methods {
            use super::*;
            $(
                $(#[doc = $doc])+
                pub struct $variant;
                impl Row for $variant {
                    const METHOD: Method = Method::$variant;
                    const NAME: &'static str = $name;
                    const DOCS: &'static str = concat!($($doc),+);
                    type Params = $params;
                    type Result = $result;
                    fn answer(
                        engine: &Engine,
                        params: Self::Params,
                        cancel: &Cancellation,
                    ) -> Result<DispatchResult<Self::Result>, EngineError> {
                        $handler(engine, params, cancel)
                    }
                    fn reply(result: Self::Result) -> Reply {
                        ($reply)(result)
                    }
                }
            )*
        }
        impl Method {
            /// This method's row, once it has one (decision 0057, step 1).
            pub(crate) const fn row(self) -> Option<Entry> {
                match self {
                    $(Self::$variant => Some(Entry::of::<methods::$variant>()),)*
                    _ => None,
                }
            }
        }
    };
}

rows! {
    /// A page of the CSL styles Ibis carries: the curated ones first, then the rest by id.
    Styles "styles": StylesRequest => StylesResult, answers::styles, Reply::Styles;
    /// The citation formats and record exports, with what each can do.
    Outputs "outputs": OutputsRequest => OutputsResult, answers::outputs, Reply::Outputs;
    /// The parameter and result schemas of one method, or of every one.
    Schema "schema": SchemaRequest => Value, answers::schema, Reply::Schema;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_answers_to_its_method_name_and_has_a_catalogue_line() {
        fn check<M: Row>() {
            assert_eq!(M::METHOD.as_str(), M::NAME);
            assert_eq!(Method::parse(M::NAME), Ok(M::METHOD));
            assert!(!M::DOCS.trim().is_empty(), "{} needs a catalogue line", M::NAME);
        }
        check::<methods::Styles>();
        check::<methods::Outputs>();
        check::<methods::Schema>();
    }

    #[test]
    fn the_catalogue_family_is_routed_by_its_rows() {
        let routed: Vec<_> = Method::ALL
            .into_iter()
            .filter(|method| method.row().is_some())
            .collect();
        assert_eq!(routed, [Method::Styles, Method::Outputs, Method::Schema]);
        for method in routed {
            let row = method.row().unwrap();
            assert!(
                (row.params)().is_object() && (row.result)().is_object(),
                "{}",
                method.as_str()
            );
        }
    }
}
