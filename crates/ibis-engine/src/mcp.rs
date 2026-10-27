//! `ibis mcp`: the engine for AI agents over the Model Context Protocol.
//!
//! Newline-delimited JSON-RPC 2.0 on stdio, as MCP's stdio transport defines,
//! pinned to protocol revision [`PROTOCOL_VERSION`]. Four read-only tools
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

/// The MCP revision this server implements. A client asking for another is
/// answered with this one, as the protocol's version negotiation requires.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
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
                Ok(message) => handle(engine, &message),
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

/// One message in, at most one message out. Notifications get no answer.
pub fn handle(engine: &Engine, message: &Value) -> Option<Value> {
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
    let result = match method {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "ibis", "title": "Ibis", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "Search the user's reference libraries and format citations. Read-only; cite items by the citation key or id that search returns.",
        }),
        "ping" => json!({}),
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
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
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
            "annotations": read_only
        },
        {
            "name": "show",
            "title": "Show a reference",
            "description": "Show one reference's bibliographic data.",
            "inputSchema": {"type": "object", "properties": {"item": item}, "required": ["item"]},
            "annotations": read_only
        },
        {
            "name": "cite",
            "title": "Format a citation",
            "description": "Format one citation for the given items, in a CSL style (default apa) or a citation key syntax such as pandoc, biblatex or typst. Returns the text with notices and losses that say what it cannot show.",
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
            "annotations": read_only
        },
        {
            "name": "styles",
            "title": "List citation styles",
            "description": "List the citation styles Ibis can format.",
            "inputSchema": {"type": "object", "properties": {}},
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
            ItemSelector::Key(key) => Some(key.clone()),
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
    // Notices and losses carry no library data; an agent needs them to know
    // what the text cannot say, as `ibis cite --json` reports them.
    match result.copy_content(items.len()) {
        Some(content) => Ok(json!({
            "text": content.text,
            "notices": result.notices,
            "losses": result.losses,
        })),
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
