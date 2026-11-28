//! The method registry shared by typed engine calls and headless front ends.
use crate::health::Checked;
use crate::outputs::OutputsResult;
use crate::request::*;
use crate::service::{
    Cancellation, DispatchResult, Engine, EntryResult, FindInResult, LibrariesResult, OutlineResult, ProjectResult,
    ReadResult, Reply, SearchResult, ShareResult, StylesResult, answers, parameters,
};
use ibis_core::contract::{CitationRequest, CitationResult, EngineError, ErrorCode, FormDetails, SnapshotObject};
use ibis_sources::SourceCapability;
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
    pub fn needs(self) -> Needs {
        self.row().needs
    }
    pub fn network(self) -> Network {
        self.row().network
    }
    pub fn palette(self) -> &'static [Selection] {
        self.row().palette
    }
}

/// What an open library must offer for a method to have anything to work on.
/// This is what the library can do, never whether the person allowed it:
/// a front end adds that (decision 0055) before it offers the method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Needs {
    /// Works with no library open.
    Nothing,
    /// Bibliographic records.
    Records,
    /// Attachments whose text can be read.
    AttachmentText,
    /// Each record's original entry as its file wrote it.
    OriginalEntries,
    /// Whatever the method it calls needs.
    AsCalled,
}
impl Needs {
    /// The capability a library declares for this, if there is one to check.
    pub fn capability(self) -> Option<SourceCapability> {
        match self {
            Self::Records => Some(SourceCapability::BibliographicRecords),
            Self::AttachmentText => Some(SourceCapability::AttachmentText),
            Self::OriginalEntries => Some(SourceCapability::OriginalEntries),
            Self::Nothing | Self::AsCalled => None,
        }
    }
}

/// Whether answering a method may reach the network, and where (decision 0008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Network {
    /// Never.
    Offline,
    /// As the method it calls does.
    AsCalled,
}

/// A selection ⌘K can offer a method for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Selection {
    /// One reference.
    Reference,
    /// Several references.
    References,
    /// One attachment of a reference.
    Attachment,
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
    /// What an open library must offer for this method to have anything to work on.
    const NEEDS: Needs;
    /// Whether answering may reach the network, and where.
    const NETWORK: Network;
    /// The selections ⌘K offers this method for; empty when it is not a ⌘K verb.
    const PALETTE: &'static [Selection];
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
    pub needs: Needs,
    pub network: Network,
    pub palette: &'static [Selection],
    pub params: fn() -> Value,
    pub result: fn() -> Value,
    pub route: fn(&Engine, &Value, &Cancellation) -> Result<DispatchResult<Reply>, EngineError>,
}
impl Entry {
    const fn of<M: Row>() -> Self {
        Self {
            docs: M::DOCS,
            needs: M::NEEDS,
            network: M::NETWORK,
            palette: M::PALETTE,
            params: || schema_for!(M::Params).into(),
            result: || schema_for!(M::Result).into(),
            route: |engine, params, cancel| {
                let params = if params.is_null() { &M::omitted() } else { params };
                Ok(engine.call::<M>(parameters(params)?, cancel)?.map_data(M::reply))
            },
        }
    }
}

/// Each row: docs, then `Type "wire-name": Params => Result { … }` with its
/// handler, how its result sits in `Reply`, what it needs, whether it goes
/// online, its ⌘K selections and, where leaving the parameters out means
/// something other than `{}`, `omitted`. `Method` keeps the rows' order,
/// which the schemas follow.
macro_rules! rows {
    ($(
        $(#[doc = $doc:literal])+
        $variant:ident $name:literal: $params:ty => $result:ty {
            answer: $handler:path,
            reply: $reply:expr,
            needs: $needs:ident,
            network: $network:ident,
            palette: [$($palette:ident),*],
            $(omitted: $omitted:tt,)?
        }
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
                    const NEEDS: Needs = Needs::$needs;
                    const NETWORK: Network = Network::$network;
                    const PALETTE: &'static [Selection] = &[$(Selection::$palette),*];
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
    Libraries "libraries": LibrariesRequest => Checked<LibrariesResult> {
        answer: answers::libraries,
        reply: Reply::Libraries,
        needs: Nothing,
        network: Offline,
        palette: [],
        omitted: {"action": "list"},
    }
    /// References that match some words, best first, across the open libraries.
    Search "search": SearchRequest => SearchResult {
        answer: answers::search,
        reply: |result| Reply::Search(Box::new(result)),
        needs: Records,
        network: Offline,
        palette: [],
    }
    /// One reference, by id or citation key, with what its library holds for it.
    Show "show": ShowRequest => Checked<SnapshotObject> {
        answer: answers::show,
        reply: |result| Reply::Show(Box::new(result)),
        needs: Records,
        network: Offline,
        palette: [Reference],
    }
    /// A citation or bibliography of references from the open libraries, in a style or a key format.
    Cite "cite": CitationRequest => Checked<CitationResult> {
        answer: answers::cite,
        reply: |result| Reply::Citation(Box::new(result)),
        needs: Records,
        network: Offline,
        palette: [],
    }
    /// A citation or export of CSL-JSON records sent with the request, without a library.
    Records "records": RecordsRequest => Checked<CitationResult> {
        answer: answers::records,
        reply: |result| Reply::Citation(Box::new(result)),
        needs: Nothing,
        network: Offline,
        palette: [],
    }
    /// Which of some keys one project `.bib` holds, read without writing to it.
    Project "project": ProjectRequest => Checked<ProjectResult> {
        answer: answers::project,
        reply: Reply::Project,
        needs: Nothing,
        network: Offline,
        palette: [],
    }
    /// One reference's original BibTeX entry, with what it needs to stand alone.
    Entry "entry": EntryRequest => Checked<EntryResult> {
        answer: answers::entry,
        reply: Reply::Entry,
        needs: OriginalEntries,
        network: Offline,
        palette: [Reference],
    }
    /// A reference's attachments: what each is and whether its text can be read.
    Outline "outline": OutlineRequest => Checked<OutlineResult> {
        answer: answers::outline,
        reply: Reply::Outline,
        needs: Records,
        network: Offline,
        palette: [Reference],
    }
    /// The text of an attachment `outline` listed, a few pages at a time.
    Read "read": ReadRequest => Checked<ReadResult> {
        answer: answers::read,
        reply: Reply::Read,
        needs: AttachmentText,
        network: Offline,
        palette: [Attachment],
    }
    /// Short passages of an attachment that hold some words, with their pages.
    FindIn "find-in": FindInRequest => Checked<FindInResult> {
        answer: answers::find_in,
        reply: Reply::FindIn,
        needs: AttachmentText,
        network: Offline,
        palette: [Attachment],
    }
    /// A pack of references to hand to someone: a reading list, a `.bib` and, when asked, their PDFs.
    Share "share": ShareRequest => Checked<ShareResult> {
        answer: answers::share,
        reply: Reply::Share,
        needs: Records,
        network: Offline,
        palette: [Reference, References],
    }
    /// A page of the CSL styles Ibis carries: the curated ones first, then the rest by id.
    Styles "styles": StylesRequest => StylesResult {
        answer: answers::styles,
        reply: Reply::Styles,
        needs: Nothing,
        network: Offline,
        palette: [],
    }
    /// The citation formats and record exports, with what each can do.
    Outputs "outputs": OutputsRequest => OutputsResult {
        answer: answers::outputs,
        reply: Reply::Outputs,
        needs: Nothing,
        network: Offline,
        palette: [],
    }
    /// The parameter and result schemas of one method, or of every one.
    Schema "schema": SchemaRequest => Vec<answers::MethodSchema> {
        answer: answers::schema,
        reply: Reply::Schema,
        needs: Nothing,
        network: Offline,
        palette: [],
    }
    /// Any other method by name, with its parameters.
    Call "call": CallRequest => Reply {
        answer: answers::call,
        reply: std::convert::identity,
        needs: AsCalled,
        network: AsCalled,
        palette: [],
    }
}

// These methods require a handshaken picker connection. In particular, the
// delivery reply is held open through the native write and its acknowledgment.
// They do not become CLI commands or public stdio methods.
macro_rules! picker_rows {
    ($($variant:ident $name:literal: $params:ty => $result:ty;)*) => {
        #[derive(Clone, Copy)]
        pub enum PickerMethod { $($variant,)* }

        impl PickerMethod {
            pub fn parse(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)*
                    _ => None,
                }
            }

            pub fn schemas() -> Value {
                serde_json::json!([$({
                    "method": $name,
                    "params": schema_for!($params),
                    "result": schema_for!($result),
                }),*])
            }
        }
    };
}

use crate::picker::*;
use crate::settings::native::{SettingsRequest, SettingsResult};
picker_rows! {
    Open "picker/open": PickerOpenRequest => PickerOpenResult;
    Act "picker/act": PickerIntent => PickerState;
    ChangeFormat "picker/format": PickerChangeFormatRequest => PickerState;
    Forms "picker/forms": PickerFormsRequest => FormDetails;
    Deliver "picker/deliver": PickerDeliverRequest => PickerDeliverResult;
    Settings "settings": SettingsRequest => SettingsResult;
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
            for (part, schema) in [("params", (row.params)()), ("result", (row.result)())] {
                let typed = ["type", "$ref", "anyOf", "oneOf", "allOf", "properties", "enum", "const"]
                    .iter()
                    .any(|keyword| schema.get(keyword).is_some());
                assert!(typed, "{} has an untyped {part} schema", method.as_str());
            }
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

    #[test]
    fn only_a_delegating_row_goes_online_as_called_and_verbs_act_on_records() {
        for method in Method::ALL {
            let delegates = method == Method::Call;
            assert_eq!(method.network() == Network::AsCalled, delegates, "{}", method.as_str());
            assert_eq!(method.needs() == Needs::AsCalled, delegates, "{}", method.as_str());
            // No row reaches the network on its own yet; one that does is a
            // reviewed change to this test and to PRIVACY.md (decision 0008).
            assert!(matches!(method.network(), Network::Offline | Network::AsCalled));
            // A ⌘K verb acts on what is selected, so it needs a library's records or text.
            if !method.palette().is_empty() {
                assert!(method.needs().capability().is_some(), "{}", method.as_str());
            }
        }
    }
}
