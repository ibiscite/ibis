//! The CLI parses arguments and presents typed engine results; no source or citation rules live here.
use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::*;
use ibis_engine::{Cancellation, DispatchResult, Engine, Method, protocol, request::*};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Read, Write},
    path::PathBuf,
};
mod toggle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Terminal,
    Plain,
}

fn command() -> Command {
    let mut root = Command::new("ibis")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Find references and cite them anywhere.")
        .subcommand_required(true)
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Print the versioned JSON envelope, including errors and findings"),
        )
        .arg(
            Arg::new("file")
                .long("file")
                .global(true)
                .value_parser(clap::value_parser!(PathBuf))
                .action(ArgAction::Append)
                .help("Open a BibTeX, BibLaTeX or CSL-JSON file; repeat for several libraries"),
        );
    root = root.arg(
        Arg::new("zotero")
            .long("zotero")
            .global(true)
            .action(ArgAction::SetTrue)
            .help("Read the active local Zotero personal library"),
    );
    for method in Method::ALL {
        let subcommand = Command::new(method.as_str());
        root = root.subcommand(match method {
            Method::Libraries => subcommand.about("List loaded libraries and source warnings"),
            Method::Search => subcommand
                .about("Search loaded records")
                .arg(Arg::new("query").required(true))
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_parser(clap::value_parser!(usize))
                        .default_value("10"),
                ),
            Method::Show => subcommand
                .about("Show one object by citation key or ItemRef")
                .arg(Arg::new("key").required_unless_present("id").conflicts_with("id"))
                .arg(Arg::new("id").long("id").value_name("ITEM_REF_JSON")),
            Method::Cite => subcommand
                .about("Render a citation through the shared engine")
                .arg(
                    Arg::new("keys")
                        .num_args(1..)
                        .required_unless_present("request")
                        .conflicts_with("request"),
                )
                .arg(
                    Arg::new("request")
                        .long("request")
                        .action(ArgAction::SetTrue)
                        .conflicts_with_all(["keys", "format", "style", "form", "locale", "bibliography"])
                        .help("Read a canonical CitationRequest JSON object from stdin"),
                )
                .arg(Arg::new("format").long("format").conflicts_with("style").value_parser([
                    "pandoc",
                    "autocite",
                    "parencite",
                    "textcite",
                    "cite",
                    "typst",
                    "keys",
                    "csl-json",
                ]))
                .arg(Arg::new("style").long("style").value_name("STYLE_ID"))
                .arg(
                    Arg::new("bibliography")
                        .long("bibliography")
                        .action(ArgAction::SetTrue)
                        .requires("style")
                        .conflicts_with("form")
                        .help("Format bibliography entries using --style"),
                )
                .arg(Arg::new("form").long("form").value_parser([
                    "normal",
                    "narrative",
                    "author-only",
                    "year-only",
                    "full",
                    "bibliography-only",
                ]))
                .arg(Arg::new("locale").long("locale").value_name("LOCALE")),
            Method::Styles => subcommand.about("List available bundled CSL styles"),
            Method::Schema => subcommand
                .about("Print generated method JSON schemas")
                .arg(Arg::new("method")),
            Method::Call => subcommand
                .about("Call any typed engine method")
                .arg(Arg::new("method").required(true))
                .arg(
                    Arg::new("params")
                        .long("params")
                        .value_name("JSON")
                        .conflicts_with("stdin"),
                )
                .arg(
                    Arg::new("stdin")
                        .long("stdin")
                        .action(ArgAction::SetTrue)
                        .help("Read method parameters from stdin"),
                ),
        });
    }
    root.subcommand(
        Command::new("serve")
            .about("Serve JSON-RPC over Content-Length framed stdio")
            .arg(
                Arg::new("stdio")
                    .long("stdio")
                    .required(true)
                    .action(ArgAction::SetTrue),
            ),
    )
    .subcommand(Command::new("mcp").about("Serve read-only search and citation tools to AI agents (MCP over stdio)"))
    .subcommand(
        Command::new("toggle")
            .about("Show or hide the desktop picker")
            .arg(Arg::new("app").long("app").value_parser(clap::value_parser!(PathBuf))),
    )
}
/// Native entry point, with streams kept explicit for script and terminal clients.
pub fn run<R: BufRead, W: Write + Send, E: Write>(
    arguments: Vec<std::ffi::OsString>,
    input: R,
    mut output: W,
    mut errors: E,
    mode: OutputMode,
) -> u8 {
    match Engine::new() {
        Ok(engine) => run_with(engine, arguments, input, output, errors, mode),
        Err(error) => emit(
            arguments.iter().any(|argument| argument == "--json"),
            Err(error),
            &mut output,
            &mut errors,
            mode,
        ),
    }
}
/// The same parser and dispatcher can run over a preloaded engine without reading user files.
pub fn run_with<R: BufRead, W: Write + Send, E: Write>(
    engine: Engine,
    arguments: Vec<std::ffi::OsString>,
    mut input: R,
    mut output: W,
    mut errors: E,
    mode: OutputMode,
) -> u8 {
    let json = arguments.iter().any(|argument| argument == "--json");
    let matches = match command().try_get_matches_from(arguments) {
        Ok(matches) => matches,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                if json {
                    return emit(
                        true,
                        Ok(DispatchResult {
                            data: json!({"help":error.to_string()}),
                            text: None,
                            source_findings: false,
                            exit_code: ibis_core::contract::ExitCode::Ok,
                        }),
                        &mut output,
                        &mut errors,
                        mode,
                    );
                }
                return if write!(output, "{error}").is_ok() { 0 } else { 70 };
            }
            return emit(
                json,
                Err(EngineError { code: ErrorCode::Usage }),
                &mut output,
                &mut errors,
                mode,
            );
        }
    };
    let engine = if matches!(matches.subcommand_name(), Some("serve" | "mcp")) {
        engine
    } else {
        engine.without_file_watches()
    };
    match execute(&engine, &matches, &mut input, &mut output) {
        Ok(Execution::Stdio(code)) => code,
        Ok(Execution::Result(result)) => emit(json, Ok(result), &mut output, &mut errors, mode),
        Err(error) if matches.subcommand_name() == Some("serve") => {
            if protocol::write_frame(&mut output, &ibis_engine::rpc::failure(None, error)).is_err() {
                70
            } else {
                error.code.exit_code().value()
            }
        }
        Err(error) => emit(json, Err(error), &mut output, &mut errors, mode),
    }
}

enum Execution {
    Result(DispatchResult),
    Stdio(u8),
}

fn execute<R: BufRead, W: Write + Send>(
    engine: &Engine,
    matches: &ArgMatches,
    input: &mut R,
    output: &mut W,
) -> Result<Execution, EngineError> {
    let (name, args) = matches.subcommand().ok_or(EngineError { code: ErrorCode::Usage })?;
    if name == "toggle" {
        toggle::run(args.get_one::<PathBuf>("app").map(PathBuf::as_path))?;
        return Ok(Execution::Result(DispatchResult {
            data: json!({"toggled":true}),
            text: None,
            source_findings: false,
            exit_code: ibis_core::contract::ExitCode::Ok,
        }));
    }
    let cancel = Cancellation::default();
    for path in matches.get_many::<PathBuf>("file").into_iter().flatten() {
        let params =
            serde_json::to_value(LibrariesRequest::OpenFile { path: path.clone() }).map_err(|_| EngineError {
                code: ErrorCode::BadInput,
            })?;
        engine.dispatch(Method::Libraries, &params, &cancel)?;
    }
    if matches.get_flag("zotero") {
        engine.dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel)?;
    }
    if name == "mcp" {
        return Ok(Execution::Stdio(match ibis_engine::mcp::serve(engine, input, output) {
            Ok(()) => 0,
            Err(_) => 70,
        }));
    }
    if name == "serve" {
        return Ok(Execution::Stdio(
            match ibis_engine::server::serve(engine.clone(), input, output) {
                Ok(()) => 0,
                Err(protocol::FrameError::Io) => 70,
                Err(_) => 3,
            },
        ));
    }
    let method = Method::parse(name).map_err(|code| EngineError { code })?;
    let params = match method {
        Method::Libraries => json!({"action":"list"}),
        Method::Search => json!({"query":args.get_one::<String>("query"),"limit":args.get_one::<usize>("limit")}),
        Method::Show => match args.get_one::<String>("id") {
            Some(id) => json!({"item":{"by":"id","value":parse_json(id.as_bytes())?}}),
            None => json!({"item":{"by":"key","value":args.get_one::<String>("key")}}),
        },
        Method::Cite if args.get_flag("request") => stdin_json(input)?,
        Method::Cite => {
            let output = if let Some(style) = args.get_one::<String>("style") {
                OutputSpec::Csl { style: style.clone() }
            } else {
                let (syntax, command) = match args.get_one::<String>("format").map(String::as_str).unwrap_or("pandoc") {
                    "autocite" => (KeySyntax::Biblatex, Some("autocite")),
                    "parencite" => (KeySyntax::Biblatex, Some("parencite")),
                    "textcite" => (KeySyntax::Biblatex, Some("textcite")),
                    "cite" => (KeySyntax::Latex, Some("cite")),
                    "typst" => (KeySyntax::Typst, None),
                    "keys" => (KeySyntax::Keys, None),
                    "csl-json" => (KeySyntax::CslJson, None),
                    _ => (KeySyntax::Pandoc, None),
                };
                OutputSpec::Key {
                    syntax,
                    command: command.map(str::to_owned),
                }
            };
            let form = if args.get_flag("bibliography") {
                CitationForm::BibliographyOnly
            } else {
                serde_json::from_value(json!(
                    args.get_one::<String>("form").map(String::as_str).unwrap_or("normal")
                ))
                .map_err(|_| EngineError { code: ErrorCode::Usage })?
            };
            let keys = args
                .get_many::<String>("keys")
                .into_iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            let result = engine.cite_keys(
                &KeyCitationRequest {
                    keys: keys.clone(),
                    output,
                    form,
                    output_locale: args.get_one::<String>("locale").cloned(),
                },
                &cancel,
            )?;
            return engine
                .finalize(DispatchResult::citation(result, keys.len())?)
                .map(Execution::Result);
        }
        Method::Styles => json!({}),
        Method::Schema => {
            json!({"method":args.get_one::<String>("method").map(|name|Method::parse(name).map_err(|code|EngineError {code})).transpose()?})
        }
        Method::Call => {
            json!({"method":Method::parse(args.get_one::<String>("method").ok_or(EngineError {code:ErrorCode::Usage})?).map_err(|code|EngineError {code})?,"params":if args.get_flag("stdin") {stdin_json(input)?} else {args.get_one::<String>("params").map(|params|parse_json(params.as_bytes())).transpose()?.unwrap_or(Value::Null)}})
        }
    };
    engine.dispatch(method, &params, &cancel).map(Execution::Result)
}
fn parse_json(bytes: &[u8]) -> Result<Value, EngineError> {
    protocol::parse_json(bytes).map_err(|_| EngineError {
        code: ErrorCode::BadInput,
    })
}
fn stdin_json(input: &mut impl BufRead) -> Result<Value, EngineError> {
    let mut bytes = vec![];
    input
        .take(protocol::MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| EngineError {
            code: ErrorCode::BadInput,
        })?;
    parse_json(&bytes)
}
fn citation_findings(data: &Value, errors: &mut impl Write) {
    let notices = data
        .get("notices")
        .and_then(|value| serde_json::from_value::<Vec<Notice>>(value.clone()).ok())
        .unwrap_or_default();
    for notice in notices {
        let message = match notice {
            Notice::NeedsDocumentContext => "this style needs the whole document for citation numbering or notes",
            Notice::ReferenceEntry => "rendered a full reference entry",
            Notice::FullFirstNote => "rendered a full first note",
            Notice::YearSuffixWithheld => "year suffixes require the whole document",
            Notice::InputLanguageUnsupported => "input language is unsupported; use typed locators",
        };
        let _ = writeln!(errors, "ibis: {message}");
    }
    if data
        .get("losses")
        .and_then(Value::as_array)
        .is_some_and(|losses| !losses.is_empty())
    {
        let _ = writeln!(
            errors,
            "ibis: some requested citation fields were omitted; inspect cite --json"
        );
    }
}
fn emit(
    json: bool,
    result: Result<DispatchResult, EngineError>,
    output: &mut impl Write,
    errors: &mut impl Write,
    mode: OutputMode,
) -> u8 {
    if result.as_ref().is_ok_and(|result| result.source_findings) {
        let _ = writeln!(
            errors,
            "ibis: source warnings or failed refresh; inspect libraries --json"
        );
    }
    if !json
        && let Ok(result) = &result
        && result.text.is_some()
    {
        citation_findings(&result.data, errors);
    }
    let (code, envelope, text) = match result.and_then(DispatchResult::bounded) {
        Ok(result) => (result.exit_code, Envelope::success(result.data), result.text),
        Err(error) => (error.code.exit_code(), Envelope::failure(error.code), None),
    };
    let encoded = if json {
        serde_json::to_value(&envelope)
            .ok()
            .and_then(|value| ibis_engine::wire::encode(&value, protocol::MAX_RESPONSE_BYTES).ok())
    } else if let Some(error) = envelope.error() {
        let _ = writeln!(errors, "ibis: {:?}", error.code);
        return code.value();
    } else if let Some(text) = text {
        Some(text.into_bytes())
    } else {
        envelope.data().and_then(|data| serde_json::to_vec_pretty(data).ok())
    };
    let Some(mut encoded) = encoded else {
        if json
            && output
                .write_all(b"{\"error\":{\"code\":\"internal\"},\"ibis\":1,\"ok\":false}\n")
                .is_err()
        {
            return 70;
        }
        return ibis_core::contract::ExitCode::Internal.value();
    };
    if !json && mode == OutputMode::Terminal {
        encoded = terminal_display(&String::from_utf8_lossy(&encoded)).into_bytes();
    }
    if output
        .write_all(&encoded)
        .and_then(|()| output.write_all(b"\n"))
        .is_err()
    {
        return 70;
    }
    code.value()
}

/// Terminal presentation escapes control sequences and invisible direction overrides;
/// canonical strings in --json output remain byte-for-byte unchanged.
fn terminal_display(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if (character.is_control() && !matches!(character, '\n' | '\t'))
            || matches!(character,'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
        {
            output.extend(character.escape_unicode());
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failure_to_write_a_bounded_json_error_is_an_internal_transport_error() {
        struct Broken;
        impl std::io::Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("synthetic transport failure"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let result = DispatchResult {
            data: json!("x".repeat(protocol::MAX_RESPONSE_BYTES + 1)),
            text: None,
            source_findings: false,
            exit_code: ibis_core::contract::ExitCode::Ok,
        };
        assert_eq!(
            emit(true, Ok(result), &mut Broken, &mut Vec::new(), OutputMode::Plain),
            70
        );
    }
    #[test]
    fn toggle_arguments_are_explicit_and_never_launched_by_parser_tests() {
        assert!(
            command()
                .try_get_matches_from(["ibis", "toggle", "--app", "Ibis Nightly"])
                .is_ok()
        );
        assert!(command().try_get_matches_from(["ibis", "toggle"]).is_ok());
        for extra in ["extra", "--at-login", "--app", "--app="] {
            assert!(command().try_get_matches_from(["ibis", "toggle", extra]).is_err());
        }
    }
}
