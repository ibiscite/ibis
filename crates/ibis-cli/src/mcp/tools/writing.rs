//! The tools `ibis mcp --allow-writes` adds (decision 0055). They only
//! propose: each answer is a proposal id and what it would change, and no
//! library changes until the person applies the proposal in Ibis. A client's
//! own approval of a tool call is never enough. Without the flag they are not
//! listed and a call is an unknown tool.
use super::*;
use ibis_core::contract::{ItemRef, LibraryId};
use ibis_engine::write::{Proposals, ProposeRequest, Source, Via, WriteError, WriteProblem};

/// What a server started with `--allow-writes` knows to record a proposal.
pub(in crate::mcp) struct Writing<'a> {
    pub proposals: &'a Proposals,
    /// The name the client gave itself.
    pub client: &'a str,
}

/// Said with every proposal, so no agent reports a change as made.
const NOT_YET: &str =
    "Nothing has changed yet. Tell the user the proposal id: they review it in Ibis and apply or reject it there.";

pub(in crate::mcp) const WRITING_TOOLS: [Tool; 4] = [
    Tool {
        name: "propose_add",
        title: "Propose adding a reference",
        description: "Propose a new reference from CSL-JSON fields. The user confirms it in Ibis; nothing is added until then.",
        input: add_input,
        output: proposed_output,
        reads_library: |_| true,
        answer: Answer::Proposes(add),
    },
    Tool {
        name: "propose_change",
        title: "Propose changing a reference",
        description: "Propose new values for fields of a reference by ref. The user confirms it in Ibis; nothing changes until then.",
        input: change_input,
        output: proposed_output,
        reads_library: |_| true,
        answer: Answer::Proposes(change),
    },
    Tool {
        name: "propose_tags",
        title: "Propose tags for a reference",
        description: "Propose tags to add to or remove from a reference by ref. The user confirms it in Ibis; nothing changes until then.",
        input: tags_input,
        output: proposed_output,
        reads_library: |_| true,
        answer: Answer::Proposes(tags),
    },
    Tool {
        name: "proposals",
        title: "List your proposals",
        description: "List the proposals from this app with their status, and the libraries propose_add can name.",
        input: proposals_input,
        output: proposals_output,
        reads_library: |_| false,
        answer: Answer::Lists(proposals),
    },
];

/// Every tool, as `fixtures/mcp-tools.json` documents them.
#[cfg(test)]
pub(super) fn full_list() -> Value {
    Value::Array(
        TOOLS
            .iter()
            .chain(reading::READING_TOOLS.iter())
            .chain(WRITING_TOOLS.iter())
            .map(entry)
            .collect(),
    )
}

fn refused(error: WriteError) -> Refusal {
    let mut refusal = Refusal::from(error);
    if let Some(advice) = ibis_engine::catalogue::text("mcp-error-proposal", None, &[("reason", &refusal.advice)]) {
        refusal.advice = advice.into();
    }
    refusal
}

fn fields_input() -> Value {
    json!({
        "type": "object",
        "description": "CSL-JSON fields, such as title, author, issued, container-title, page and DOI. No note, file or id.",
        "minProperties": 1
    })
}

fn add_input() -> Value {
    let mut fields = fields_input();
    fields["required"] = json!(["type", "title"]);
    json!({
        "type": "object",
        "properties": {
            "fields": fields,
            "library": {"type": "string", "description": "A library from proposals; needed when several are open."}
        },
        "required": ["fields"],
        "additionalProperties": false
    })
}

fn change_input() -> Value {
    json!({
        "type": "object",
        "properties": {"ref": {"type": "string", "description": "A ref from search."}, "fields": fields_input()},
        "required": ["ref", "fields"],
        "additionalProperties": false
    })
}

fn tags_input() -> Value {
    let tags = json!({"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": ibis_engine::write::MAX_TAG_CHARS}, "maxItems": ibis_engine::write::MAX_TAGS});
    json!({
        "type": "object",
        "properties": {"ref": {"type": "string", "description": "A ref from search."}, "add": tags, "remove": tags},
        "required": ["ref"],
        "additionalProperties": false
    })
}

fn proposals_input() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

fn proposed_output() -> Value {
    json!({
        "type": "object",
        "properties": {
            "proposal": {"type": "string"},
            "status": {"type": "string"},
            "summary": {"type": "string"},
            "bibtex": {"type": "string"},
            "next": {"type": "string"},
            "provenance": {"type": "string"}
        },
        "required": ["proposal", "status", "summary", "next", "provenance"]
    })
}

fn proposals_output() -> Value {
    json!({
        "type": "object",
        "properties": {
            "proposals": {"type": "array", "items": {"type": "object", "required": ["proposal", "status", "summary"]}},
            "libraries": {"type": "array", "items": {"type": "object", "required": ["library", "kind", "references"]}},
            "provenance": {"type": "string"}
        },
        "required": ["proposals", "libraries", "provenance"]
    })
}

fn source(writing: &Writing<'_>) -> Source {
    Source {
        via: Via::Mcp,
        client: writing.client.to_owned(),
    }
}

fn fields(arguments: &Map<String, Value>) -> Result<Map<String, Value>, Refusal> {
    match arguments.get("fields") {
        Some(Value::Object(fields)) if !fields.is_empty() => Ok(fields.clone()),
        _ => Err(Refusal::bad(ToolProblem::Arguments, &[])),
    }
}

/// The record a ref names, as long as search saw it as it is.
fn item(engine: &Engine, arguments: &Map<String, Value>, cancel: &Cancellation) -> Result<ItemRef, Refusal> {
    Ok(selected(engine, arguments, cancel)?.value.object.id().clone())
}

/// A proposal as an agent reads it: what it changes in words, never the
/// library file's own text.
fn proposed(engine: &Engine, writing: &Writing<'_>, request: ProposeRequest) -> Result<Value, Refusal> {
    let proposal = engine
        .propose(writing.proposals, source(writing), request)
        .map_err(refused)?;
    let mut answer = json!({
        "proposal": proposal.id,
        "status": proposal.status,
        "summary": ibis_engine::catalogue::message(&ibis_engine::write::summary(&proposal), ibis_engine::catalogue::Audience::Agent),
        "next": NOT_YET,
        "provenance": PROVENANCE,
    });
    if let ibis_engine::write::Change::Add { key, fields } = &proposal.change
        && let Some(entry) = ibis_core::syntax::bibtex_entry(key, &CitableFields::new(fields.clone()))
    {
        answer["bibtex"] = json!(entry);
    }
    Ok(answer)
}

fn add(
    engine: &Engine,
    writing: &Writing<'_>,
    arguments: &Map<String, Value>,
    _: &Cancellation,
) -> Result<Value, Refusal> {
    let library = match arguments.get("library") {
        None => None,
        Some(Value::String(library)) => {
            Some(LibraryId::new(library).map_err(|_| refused(WriteProblem::UnknownLibrary.into()))?)
        }
        Some(_) => return Err(Refusal::bad(ToolProblem::Arguments, &[])),
    };
    let fields = fields(arguments)?;
    proposed(engine, writing, ProposeRequest::Add { library, fields })
}

fn change(
    engine: &Engine,
    writing: &Writing<'_>,
    arguments: &Map<String, Value>,
    cancel: &Cancellation,
) -> Result<Value, Refusal> {
    let item = item(engine, arguments, cancel)?;
    let fields = fields(arguments)?;
    proposed(engine, writing, ProposeRequest::Fields { item, fields })
}

fn tags(
    engine: &Engine,
    writing: &Writing<'_>,
    arguments: &Map<String, Value>,
    cancel: &Cancellation,
) -> Result<Value, Refusal> {
    let list = |name: &str| -> Result<Vec<String>, Refusal> {
        match arguments.get(name) {
            None => Ok(Vec::new()),
            Some(Value::Array(tags)) => tags
                .iter()
                .map(|tag| tag.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| Refusal::bad(ToolProblem::Arguments, &[])),
            Some(_) => Err(Refusal::bad(ToolProblem::Arguments, &[])),
        }
    };
    let (add, remove) = (list("add")?, list("remove")?);
    let item = item(engine, arguments, cancel)?;
    proposed(engine, writing, ProposeRequest::Tags { item, add, remove })
}

fn proposals(
    engine: &Engine,
    writing: &Writing<'_>,
    _: &Map<String, Value>,
    _: &Cancellation,
) -> Result<Value, Refusal> {
    let mine: Vec<Value> = writing
        .proposals
        .list()
        .map_err(Refusal::from)?
        .into_iter()
        .filter(|proposal| proposal.source.client == ibis_engine::write::client_name(writing.client))
        .take(MAX_HITS)
        .map(|proposal| {
            json!({
                "proposal": proposal.id,
                "status": proposal.status,
                "summary": ibis_engine::catalogue::message(&ibis_engine::write::summary(&proposal), ibis_engine::catalogue::Audience::Agent),
            })
        })
        .collect();
    let libraries: Vec<Value> = engine
        .libraries()?
        .libraries
        .into_iter()
        // Only libraries that can take a proposal, by kind, never by file name.
        .filter(|library| engine.proposal_target(&library.id).is_ok_and(|target| target.is_some()))
        .map(|library| json!({"library": library.id, "kind": "bibtex", "references": library.objects}))
        .collect();
    Ok(json!({"proposals": mine, "libraries": libraries, "provenance": PROVENANCE}))
}
