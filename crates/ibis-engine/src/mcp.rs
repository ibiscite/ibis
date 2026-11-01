//! `ibis mcp`: the engine for AI agents over the Model Context Protocol.
//!
//! Newline-delimited JSON-RPC 2.0 on stdio, as MCP's stdio transport defines.
//! Requests that name revision [`PROTOCOL_VERSION`] in `_meta` are served
//! statelessly; older clients that open with `initialize` get one of
//! [`LEGACY_VERSIONS`], as the revision's dual-era rules allow. Four read-only tools
//! (search, show, cite, styles) call the same engine methods as the command
//! line. Agents see records only as [`citable_fields`] leaves them: never notes
//! or annotations, a record's `note` or `annote` fields, file locations,
//! private links or other libraries' data. Long values are cut short and every
//! answer has a size bound. Nothing here opens a network connection.

use crate::request::{ItemSelector, KeyCitationRequest, SearchRequest, ShowRequest};
use crate::{Cancellation, Engine, protocol};
use ibis_core::citable::{citable_fields, is_variable};
use ibis_core::contract::{
    CitationForm, CitationItem, CitationRequest, EngineError, ErrorCode, ItemRef, KeySyntax, LibraryObject, OutputSpec,
    Record,
};
use serde_json::{Map, Value, json};
use std::io::{self, BufRead, Read, Write};

/// The current MCP revision: no handshake, the version travels in each request's `_meta`.
pub const PROTOCOL_VERSION: &str = "2026-07-28";
/// Handshake revisions still answered after `initialize`, newest first.
pub const LEGACY_VERSIONS: [&str; 2] = ["2025-11-25", "2025-06-18"];
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const SERVER_INFO_KEY: &str = "io.modelcontextprotocol/serverInfo";
/// The tool list only changes with the binary, so clients may keep it for an hour.
const LIST_TTL_MS: u64 = 3_600_000;
const INSTRUCTIONS: &str = "Search the user's reference libraries and format citations. Read-only; cite items by the citation key or id that search returns.";
/// Search results returned to an agent at most.
pub const MAX_HITS: usize = 25;
/// Characters kept of any one text value.
pub const MAX_TEXT: usize = 1_000;
/// Entries kept of any one list (authors, keywords).
pub const MAX_LIST: usize = 20;
/// Bytes of JSON one tool answer may carry. Search returns fewer hits rather
/// than more; anything else larger is refused.
pub const MAX_ANSWER: usize = 256 * 1024;
/// Key syntaxes `cite` offers: those the engine renders from keys alone.
/// CSL-JSON is left out because it would carry whole records.
const SYNTAXES: [KeySyntax; 6] = [
    KeySyntax::Pandoc,
    KeySyntax::Biblatex,
    KeySyntax::Natbib,
    KeySyntax::Latex,
    KeySyntax::Typst,
    KeySyntax::Keys,
];

/// Serves until the client closes stdin.
pub fn serve<R: BufRead, W: Write>(engine: &Engine, mut reader: R, mut writer: W) -> io::Result<()> {
    let mut session = Session::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .by_ref()
            .take(protocol::MAX_REQUEST_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(());
        }
        let reply = if line.len() > protocol::MAX_REQUEST_BYTES && !line.ends_with(b"\n") {
            // Skip the rest of an oversized message, then answer it once.
            skip_line(&mut reader)?;
            Some(error(Value::Null, -32600, "Request too large"))
        } else {
            let text = line.trim_ascii();
            if text.is_empty() {
                continue;
            }
            match protocol::parse_json(text) {
                Ok(message) => handle(engine, &mut session, &message),
                Err(_) => Some(error(Value::Null, -32700, "Parse error")),
            }
        };
        if let Some(reply) = reply {
            let mut bytes = serde_json::to_vec(&reply).map_err(io::Error::other)?;
            bytes.push(b'\n');
            writer.write_all(&bytes)?;
            writer.flush()?;
        }
    }
}

fn skip_line(reader: &mut impl BufRead) -> io::Result<()> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(());
        }
        match buffer.iter().position(|&byte| byte == b'\n') {
            Some(end) => {
                reader.consume(end + 1);
                return Ok(());
            }
            None => {
                let length = buffer.len();
                reader.consume(length);
            }
        }
    }
}

/// What one stdio process remembers: only whether an older client opened it with
/// `initialize`. Requests that name the 2026-07-28 revision never depend on it.
#[derive(Debug, Default)]
pub struct Session {
    legacy: bool,
}

const CAPABILITIES_KEY: &str = "io.modelcontextprotocol/clientCapabilities";

/// One message in, at most one message out. Notifications get no answer.
pub fn handle(engine: &Engine, session: &mut Session, message: &Value) -> Option<Value> {
    let Some(object) = message.as_object() else {
        // Batches are not part of this revision.
        return Some(error(Value::Null, -32600, "Invalid Request"));
    };
    let id = object.get("id").cloned();
    let valid_id = id
        .as_ref()
        .is_none_or(|id| id.is_string() || id.is_i64() || id.is_u64());
    let reply_id = id.clone().filter(|_| valid_id).unwrap_or(Value::Null);
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error(reply_id, -32600, "Invalid Request"));
    }
    let method = match object.get("method") {
        Some(Value::String(method)) => method.as_str(),
        // A response to a request we never sent.
        None if object.contains_key("result") || object.contains_key("error") => return None,
        // Anything else is malformed; answer it so the client does not wait.
        _ => return Some(error(reply_id, -32600, "Invalid Request")),
    };
    let id = id?;
    if !valid_id {
        return Some(error(Value::Null, -32600, "Invalid Request"));
    }
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    // A request that names its revision is a stateless 2026-07-28 request and must
    // carry the fields that revision requires; one that does not is served only to a
    // client that opened this process with `initialize`.
    let meta = params.get("_meta");
    let modern = match meta.and_then(|meta| meta.get(VERSION_KEY)) {
        // Older clients may ping before their handshake finishes.
        None if method == "initialize" || method == "ping" || session.legacy => false,
        None => return Some(error(id, -32602, "Invalid params: name the protocol version in _meta")),
        Some(Value::String(version)) if version == PROTOCOL_VERSION => {
            if !meta
                .and_then(|meta| meta.get(CAPABILITIES_KEY))
                .is_some_and(Value::is_object)
            {
                return Some(error(id, -32602, "Invalid params: _meta needs client capabilities"));
            }
            true
        }
        Some(Value::String(requested)) => {
            return Some(json!({"jsonrpc": "2.0", "id": id, "error": {
                "code": -32022,
                "message": "Unsupported protocol version",
                "data": {"supported": supported_versions(), "requested": requested},
            }}));
        }
        Some(_) => {
            return Some(error(
                id,
                -32602,
                "Invalid params: the protocol version must be a string",
            ));
        }
    };
    let mut result = match method {
        "server/discover" => json!({
            "supportedVersions": supported_versions(),
            "capabilities": {"tools": {}},
            "instructions": INSTRUCTIONS,
            "ttlMs": LIST_TTL_MS,
            "cacheScope": "public",
        }),
        "initialize" if !modern => {
            let requested = params.get("protocolVersion").and_then(Value::as_str);
            // Only a complete handshake opens the process to requests without a revision.
            if requested.is_none()
                || !params.get("capabilities").is_some_and(Value::is_object)
                || !params.get("clientInfo").is_some_and(Value::is_object)
            {
                return Some(error(
                    id,
                    -32602,
                    "Invalid params: initialize needs protocolVersion, capabilities and clientInfo",
                ));
            }
            session.legacy = true;
            json!({
                "protocolVersion": LEGACY_VERSIONS
                    .into_iter()
                    .find(|version| Some(*version) == requested)
                    .unwrap_or(LEGACY_VERSIONS[0]),
                "capabilities": {"tools": {}},
                "serverInfo": server_info(),
                "instructions": INSTRUCTIONS,
            })
        }
        "ping" if !modern => json!({}),
        "tools/list" if modern => json!({"tools": tools(), "ttlMs": LIST_TTL_MS, "cacheScope": "public"}),
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str);
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match name {
                Some(name) if TOOLS.contains(&name) => call(engine, name, &arguments),
                _ => return Some(error(id, -32602, "Unknown tool")),
            }
        }
        _ => return Some(error(id, -32601, "Method not found")),
    };
    if modern || method == "server/discover" {
        result["resultType"] = json!("complete");
        result["_meta"] = json!({SERVER_INFO_KEY: server_info()});
    }
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn supported_versions() -> Vec<&'static str> {
    std::iter::once(PROTOCOL_VERSION).chain(LEGACY_VERSIONS).collect()
}

fn server_info() -> Value {
    json!({"name": "ibis", "title": "Ibis", "version": env!("CARGO_PKG_VERSION")})
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

const TOOLS: [&str; 4] = ["search", "show", "cite", "styles"];

fn tools() -> Value {
    let read_only =
        json!({"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false});
    let item = json!({
        "type": "object",
        "description": "An item from search: its citation key, or its id when it has no key.",
        "properties": {
            "key": {"type": "string"},
            "id": {"type": "object", "properties": {"library": {"type": "string"}, "key": {"type": "string"}}, "required": ["library", "key"]}
        }
    });
    // Output schemas promise only what every successful answer holds; values
    // copied from a library keep whatever JSON type the library gave them.
    let summary = json!({
        "type": "object",
        "properties": {
            "key": {"type": ["string", "null"]},
            "id": {"type": "object", "properties": {"library": {"type": "string"}, "key": {"type": "string"}}, "required": ["library", "key"]},
            "type": {}, "title": {}, "year": {}, "container": {}, "doi": {},
            "authors": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["key", "id", "type", "title", "authors", "year", "container", "doi"]
    });
    let mut shown = summary.clone();
    shown["properties"]["csl"] = json!({"type": "object"});
    shown["required"] = json!([
        "key",
        "id",
        "type",
        "title",
        "authors",
        "year",
        "container",
        "doi",
        "csl"
    ]);
    json!([
        {
            "name": "search",
            "title": "Search references",
            "description": "Search the user's libraries by author, title, year or citation key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_HITS, "default": 10}
                },
                "required": ["query"]
            },
            "outputSchema": {
                "type": "object",
                "properties": {
                    "hits": {"type": "array", "items": summary},
                    "complete": {"type": "boolean", "description": "False when the answer size bound cut the list short."}
                },
                "required": ["hits", "complete"]
            },
            "annotations": read_only
        },
        {
            "name": "show",
            "title": "Show a reference",
            "description": "Show one reference's bibliographic data.",
            "inputSchema": {"type": "object", "properties": {"item": item}, "required": ["item"]},
            "outputSchema": shown,
            "annotations": read_only
        },
        {
            "name": "cite",
            "title": "Format a citation",
            "description": "Format one citation for the given items, in a CSL style (default apa) or a citation key syntax such as pandoc, biblatex or typst. Returns the text with item warnings, notices and losses that say what needs checking or cannot be shown.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "items": {"type": "array", "items": item, "minItems": 1},
                    "style": {"type": "string", "description": "A style id from the styles tool."},
                    "syntax": {"type": "string", "enum": SYNTAXES.map(syntax_name)},
                    "form": {"type": "string", "enum": ["normal", "narrative", "author-only", "year-only", "full", "bibliography-only"]}
                },
                "required": ["items"]
            },
            "outputSchema": {
                "type": "object",
                "properties": {
                    "text": {"type": "string"},
                    "notices": {"type": "array"},
                    "losses": {"type": "array"},
                    "warnings": {"type": "array"}
                },
                "required": ["text", "notices", "losses"]
            },
            "annotations": read_only
        },
        {
            "name": "styles",
            "title": "List citation styles",
            "description": "List the citation styles Ibis can format.",
            "inputSchema": {"type": "object", "properties": {}},
            "outputSchema": {
                "type": "object",
                "properties": {
                    "styles": {
                        "type": "array",
                        "items": {"type": "object", "properties": {"id": {"type": "string"}, "title": {"type": "string"}}, "required": ["id", "title"]}
                    }
                },
                "required": ["styles"]
            },
            "annotations": read_only
        }
    ])
}

fn syntax_name(syntax: KeySyntax) -> String {
    serde_json::to_value(syntax)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// A tool's answer. Engine failures are tool errors the agent can read, with
/// the engine's fixed error code and nothing from the library.
fn call(engine: &Engine, name: &str, arguments: &Value) -> Value {
    let outcome = match name {
        "search" => search(engine, arguments),
        "show" => show(engine, arguments),
        "cite" => cite(engine, arguments),
        _ => engine
            .styles()
            .and_then(|styles| serde_json::to_value(styles).map_err(|_| internal())),
    };
    let outcome = outcome.and_then(|data| {
        let text = data.to_string();
        if text.len() > MAX_ANSWER {
            return Err(EngineError {
                code: ErrorCode::ResourceLimit,
            });
        }
        Ok((text, data))
    });
    match outcome {
        Ok((text, data)) => json!({
            "content": [{"type": "text", "text": text}],
            "structuredContent": data,
            "isError": false
        }),
        Err(EngineError { code }) => {
            let code = serde_json::to_value(code).unwrap_or(Value::Null);
            json!({
                "content": [{"type": "text", "text": format!("error: {}", code.as_str().unwrap_or("internal"))}],
                "isError": true
            })
        }
    }
}

fn internal() -> EngineError {
    EngineError {
        code: ErrorCode::Internal,
    }
}

fn bad_input() -> EngineError {
    EngineError {
        code: ErrorCode::BadInput,
    }
}

fn search(engine: &Engine, arguments: &Value) -> Result<Value, EngineError> {
    let query = arguments.get("query").and_then(Value::as_str).ok_or_else(bad_input)?;
    let limit = match arguments.get("limit") {
        None => 10,
        Some(limit) => limit
            .as_u64()
            .and_then(|limit| usize::try_from(limit).ok())
            .filter(|limit| (1..=MAX_HITS).contains(limit))
            .ok_or_else(bad_input)?,
    };
    let result = engine.search(
        &SearchRequest {
            interpret_locator: false,
            query: query.to_owned(),
            limit,
            libraries: None,
        },
        &Cancellation::default(),
    )?;
    let mut hits = Vec::new();
    let mut size = 0;
    let mut complete = true;
    for hit in &result.hits {
        let LibraryObject::Record(record) = &hit.item.object else {
            continue;
        };
        let hit = summary(&citable(record));
        size += hit.to_string().len() + 1;
        // Leave room for the envelope; a shorter list is still a true answer.
        if size > MAX_ANSWER - 1024 {
            complete = false;
            break;
        }
        hits.push(hit);
    }
    Ok(json!({"hits": hits, "complete": complete}))
}

fn show(engine: &Engine, arguments: &Value) -> Result<Value, EngineError> {
    let item = selector(arguments.get("item").ok_or_else(bad_input)?)?;
    let shown = engine.show(&ShowRequest { item })?;
    match &shown.object {
        LibraryObject::Record(record) => {
            let citable = citable(record);
            let mut data = summary(&citable);
            let mut csl = citable.csl;
            csl.retain(|field, _| is_variable(field));
            data["csl"] = shorten(&Value::Object(csl));
            Ok(data)
        }
        // Notes, annotations and the rest are not shared with agents.
        _ => Err(EngineError {
            code: ErrorCode::NotFound,
        }),
    }
}

fn cite(engine: &Engine, arguments: &Value) -> Result<Value, EngineError> {
    let items = arguments.get("items").and_then(Value::as_array).ok_or_else(bad_input)?;
    if items.is_empty() || items.len() > ibis_core::syntax::MAX_CITATION_ITEMS {
        return Err(bad_input());
    }
    let output = match (arguments.get("syntax"), arguments.get("style")) {
        (Some(_), Some(_)) => return Err(bad_input()),
        (Some(syntax), None) => OutputSpec::Key {
            syntax: serde_json::from_value(syntax.clone())
                .ok()
                .filter(|syntax| SYNTAXES.contains(syntax))
                .ok_or_else(bad_input)?,
            command: None,
        },
        (None, style) => OutputSpec::Csl {
            style: style
                .map_or(Some("apa"), Value::as_str)
                .ok_or_else(bad_input)?
                .to_owned(),
        },
    };
    let form: CitationForm = match arguments.get("form") {
        Some(form) => serde_json::from_value(form.clone()).map_err(|_| bad_input())?,
        None => CitationForm::default(),
    };
    let cancel = Cancellation::default();
    let selectors = items.iter().map(selector).collect::<Result<Vec<_>, _>>()?;
    let result = if let Some(keys) = selectors
        .iter()
        .map(|selector| match selector {
            ItemSelector::Key(key) => Some(key.as_str().into()),
            ItemSelector::Id(_) => None,
        })
        .collect::<Option<Vec<_>>>()
    {
        engine.cite_keys(
            &KeyCitationRequest {
                keys,
                output,
                form,
                output_locale: None,
            },
            &cancel,
        )?
    } else {
        let items = selectors
            .into_iter()
            .map(|selector| {
                let item = match selector {
                    ItemSelector::Id(id) => id,
                    key @ ItemSelector::Key(_) => engine.show(&ShowRequest { item: key })?.object.id().clone(),
                };
                Ok(CitationItem {
                    item,
                    expected_cite: None,
                    form: None,
                    locator: None,
                    prefix: vec![],
                    suffix: vec![],
                    suppress_author: false,
                    annotation: None,
                    expected_annotation_cite: None,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        engine.cite(
            &CitationRequest {
                items,
                form,
                keep_order: matches!(output, OutputSpec::Key { .. }),
                output,
                input_language: None,
                output_locale: None,
            },
            &cancel,
        )?
    };
    // Only a complete citation is handed on; anything less is an error the
    // agent can see, never a partial text it might paste.
    // Fixed-code warnings, notices and losses carry no library data; an agent needs them to know
    // what the text cannot say, as `ibis cite --json` reports them.
    match result.copy_content(items.len()) {
        Some(content) => {
            let mut answer = json!({
            "text": content.text,
            "notices": result.notices,
            "losses": result.losses,
            });
            if !result.warnings.is_empty() {
                answer["warnings"] = json!(result.warnings);
            }
            Ok(answer)
        }
        None => Err(bad_input()),
    }
}

fn selector(item: &Value) -> Result<ItemSelector, EngineError> {
    match (item.get("key"), item.get("id")) {
        (Some(Value::String(key)), None) => Ok(ItemSelector::Key(key.clone())),
        (None, Some(id)) => serde_json::from_value::<ItemRef>(id.clone())
            .map(ItemSelector::Id)
            .map_err(|_| bad_input()),
        _ => Err(bad_input()),
    }
}

/// A record's fields as formatting sees them, with its key and id.
struct Citable<'a> {
    record: &'a Record,
    csl: Map<String, Value>,
}

fn citable(record: &Record) -> Citable<'_> {
    let mut csl = record
        .csl
        .iter()
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect();
    citable_fields(&mut csl);
    Citable { record, csl }
}

/// What an agent needs to recognise and cite a record.
fn summary(citable: &Citable<'_>) -> Value {
    let Citable { record, csl } = citable;
    let field = |name: &str| csl.get(name).map(shorten).unwrap_or(Value::Null);
    let authors: Vec<Value> = csl
        .get("author")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_LIST)
        .map(|name| {
            let part = |part: &str| name.get(part).and_then(Value::as_str).unwrap_or_default();
            let text = match (part("family"), part("given"), part("literal")) {
                (family, "", "") => family.to_owned(),
                ("", "", literal) => literal.to_owned(),
                (family, given, _) => format!("{family}, {given}"),
            };
            Value::String(cut(&text))
        })
        .collect();
    let year = csl
        .get("issued")
        .and_then(|issued| {
            issued
                .get("date-parts")
                .and_then(|parts| parts.get(0))
                .and_then(|first| first.get(0))
                .or_else(|| issued.get("raw"))
        })
        .map(shorten)
        .unwrap_or(Value::Null);
    json!({
        "key": record.citation_key().ok().flatten(),
        "id": record.id,
        "type": field("type"),
        "title": field("title"),
        "authors": authors,
        "year": year,
        "container": field("container-title"),
        "doi": field("DOI"),
    })
}

fn shorten(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(cut(text)),
        Value::Array(values) => Value::Array(values.iter().take(MAX_LIST).map(shorten).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), shorten(value)))
                .collect::<Map<_, _>>(),
        ),
        other => other.clone(),
    }
}

fn cut(text: &str) -> String {
    match text.char_indices().nth(MAX_TEXT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}
