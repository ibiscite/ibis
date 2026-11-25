//! The method registry shared by typed engine calls and headless front ends.
use crate::health::Checked;
use crate::outputs::OutputsResult;
use crate::request::*;
use crate::service::{
    Cancellation, DispatchResult, Engine, EntryResult, FindInResult, LibrariesResult, OutlineResult, ProjectResult,
    ReadResult, Reply, SearchResult, ShareResult, StylesResult, answers, parameters,
};
use ibis_core::contract::{CitationRequest, CitationResult, EngineError, ErrorCode, SnapshotObject};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

impl Method {
    pub fn parse(name: &str) -> Result<Self, ErrorCode> {
        Self::ALL
            .into_iter()
            .find(|method| method.as_str() == name)
            .ok_or(ErrorCode::Usage)
    }
    /// The method's catalogue line.
    pub fn docs(self) -> &'static str {
        self.row().docs
    }
}

/// One public method, declared once in the `rows!` table (decision 0057): its wire
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
    /// The parameters a request means when it leaves them out.
    fn omitted() -> Value {
        Value::Object(serde_json::Map::new())
    }
}

/// What dispatch and the schemas read from a row, without its types.
#[derive(Clone, Copy)]
pub(crate) struct Entry {
    pub docs: &'static str,
    pub params: fn() -> Value,
    pub result: fn() -> Value,
    pub route: fn(&Engine, &Value, &Cancellation) -> Result<DispatchResult<Reply>, EngineError>,
}
impl Entry {
    const fn of<M: Row>() -> Self {
        Self {
            docs: M::DOCS,
            params: || schema_for!(M::Params).into(),
            result: || schema_for!(M::Result).into(),
            route: |engine, params, cancel| {
                let params = if params.is_null() { &M::omitted() } else { params };
                Ok(engine.call::<M>(parameters(params)?, cancel)?.map_data(M::reply))
            },
        }
    }
}

/// Each row: docs, then `Type "wire-name": Params => Result, handler, reply`,
/// and, where leaving the parameters out means something other than `{}`,
/// `omitted {…}`. `Method` keeps the rows' order, which the schemas follow.
macro_rules! rows {
    ($(
        $(#[doc = $doc:literal])+
        $variant:ident $name:literal: $params:ty => $result:ty, $handler:path, $reply:expr
        $(, omitted $omitted:tt)?;
    )*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub enum Method {
            $(#[serde(rename = $name)] $variant,)*
        }
        impl Method {
            pub const ALL: [Self; [$($name),*].len()] = [$(Self::$variant),*];
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }
            pub(crate) const fn row(self) -> Entry {
                match self {
                    $(Self::$variant => Entry::of::<methods::$variant>(),)*
                }
            }
        }
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
                    $(fn omitted() -> Value {
                        serde_json::json!($omitted)
                    })?
                }
            )*
        }
    };
}

rows! {
    /// The libraries Ibis has open and their health; also opens one, reads them again or asks Zotero about its own.
    Libraries "libraries": LibrariesRequest => Checked<LibrariesResult>, answers::libraries, Reply::Libraries,
        omitted {"action": "list"};
    /// References that match some words, best first, across the open libraries.
    Search "search": SearchRequest => SearchResult, answers::search, |result| Reply::Search(Box::new(result));
    /// One reference, by id or citation key, with what its library holds for it.
    Show "show": ShowRequest => Checked<SnapshotObject>, answers::show, |result| Reply::Show(Box::new(result));
    /// A citation or bibliography of references from the open libraries, in a style or a key format.
    Cite "cite": CitationRequest => Checked<CitationResult>, answers::cite, |result| Reply::Citation(Box::new(result));
    /// A citation or export of CSL-JSON records sent with the request, without a library.
    Records "records": RecordsRequest => Checked<CitationResult>, answers::records,
        |result| Reply::Citation(Box::new(result));
    /// Which of some keys one project `.bib` holds, read without writing to it.
    Project "project": ProjectRequest => Checked<ProjectResult>, answers::project, Reply::Project;
    /// One reference's original BibTeX entry, with what it needs to stand alone.
    Entry "entry": EntryRequest => Checked<EntryResult>, answers::entry, Reply::Entry;
    /// A reference's attachments: what each is and whether its text can be read.
    Outline "outline": OutlineRequest => Checked<OutlineResult>, answers::outline, Reply::Outline;
    /// The text of an attachment `outline` listed, a few pages at a time.
    Read "read": ReadRequest => Checked<ReadResult>, answers::read, Reply::Read;
    /// Short passages of an attachment that hold some words, with their pages.
    FindIn "find-in": FindInRequest => Checked<FindInResult>, answers::find_in, Reply::FindIn;
    /// A pack of references to hand to someone: a reading list, a `.bib` and, when asked, their PDFs.
    Share "share": ShareRequest => Checked<ShareResult>, answers::share, Reply::Share;
    /// A page of the CSL styles Ibis carries: the curated ones first, then the rest by id.
    Styles "styles": StylesRequest => StylesResult, answers::styles, Reply::Styles;
    /// The citation formats and record exports, with what each can do.
    Outputs "outputs": OutputsRequest => OutputsResult, answers::outputs, Reply::Outputs;
    /// The parameter and result schemas of one method, or of every one.
    Schema "schema": SchemaRequest => Vec<answers::MethodSchema>, answers::schema, Reply::Schema;
    /// Any other method by name, with its parameters.
    Call "call": CallRequest => Reply, answers::call, std::convert::identity;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_method_answers_to_its_name_and_has_a_catalogue_line() {
        for method in Method::ALL {
            assert_eq!(Method::parse(method.as_str()), Ok(method));
            assert_eq!(serde_json::to_value(method).unwrap(), method.as_str());
            assert!(
                !method.docs().trim().is_empty(),
                "{} needs a catalogue line",
                method.as_str()
            );
            let row = method.row();
            assert!(
                (row.params)().is_object() && (row.result)().is_object(),
                "{}",
                method.as_str()
            );
        }
        assert_eq!(Method::parse("nothing"), Err(ErrorCode::Usage));
    }

    #[test]
    fn a_typed_row_names_its_method() {
        fn check<M: Row>() {
            assert_eq!(M::METHOD.as_str(), M::NAME);
            assert_eq!(M::METHOD.docs(), M::DOCS);
        }
        check::<methods::Libraries>();
        check::<methods::FindIn>();
        check::<methods::Call>();
    }
}
