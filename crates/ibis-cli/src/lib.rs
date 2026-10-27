//! The CLI parses arguments and presents typed engine results; no source or citation rules live here.
use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::*;
use ibis_engine::{Cancellation, DispatchResult, Engine, Method, protocol, request::*};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Read, Write},
    path::PathBuf,
};
mod human;
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
            Err(error.into()),
            None,
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
                        None,
                        &mut output,
                        &mut errors,
                        mode,
                    );
                }
                return if write!(output, "{error}").is_ok() { 0 } else { 70 };
            }
            // clap names the argument and prints the usage line; keep its words.
            let message = error.to_string();
            return emit(
                json,
                Err(Failure::new(
                    ErrorCode::Usage,
                    message.trim_start_matches("error: ").trim_end(),
                )),
                None,
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
        Ok(Execution::Result(result, citing)) => {
            emit(json, Ok(result), citing.as_ref(), &mut output, &mut errors, mode)
        }
        Err(failure) if matches.subcommand_name() == Some("serve") => {
            let error = EngineError { code: failure.code };
            if protocol::write_frame(&mut output, &ibis_engine::rpc::failure(None, error)).is_err() {
                70
            } else {
                error.code.exit_code().value()
            }
        }
        Err(failure) => emit(json, Err(failure), None, &mut output, &mut errors, mode),
    }
}

enum Execution {
    Result(DispatchResult, Option<human::Citing>),
    Stdio(u8),
}

/// A failure keeps its contract code, and so its exit code; the message is for people only.
struct Failure {
    code: ErrorCode,
    message: Option<String>,
}
impl Failure {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: Some(message.into()),
        }
    }
}
impl From<EngineError> for Failure {
    fn from(error: EngineError) -> Self {
        Self {
            code: error.code,
            message: None,
        }
    }
}
/// Attach a sentence to an engine error without changing its code.
fn explain<T>(result: Result<T, EngineError>, message: impl FnOnce(ErrorCode) -> String) -> Result<T, Failure> {
    result.map_err(|error| Failure::new(error.code, message(error.code)))
}

fn execute<R: BufRead, W: Write + Send>(
    engine: &Engine,
    matches: &ArgMatches,
    input: &mut R,
    output: &mut W,
) -> Result<Execution, Failure> {
    let (name, args) = matches.subcommand().ok_or(EngineError { code: ErrorCode::Usage })?;
    if name == "toggle" {
        explain(
            toggle::run(args.get_one::<PathBuf>("app").map(PathBuf::as_path)),
            |_| "could not start the Ibis app; install it, or give its location with --app <path>.".into(),
        )?;
        return Ok(Execution::Result(
            DispatchResult {
                data: json!({"toggled":true}),
                text: None,
                source_findings: false,
                exit_code: ibis_core::contract::ExitCode::Ok,
            },
            None,
        ));
    }
    let cancel = Cancellation::default();
    let mut names = std::collections::BTreeMap::new();
    for path in matches.get_many::<PathBuf>("file").into_iter().flatten() {
        let params =
            serde_json::to_value(LibrariesRequest::OpenFile { path: path.clone() }).map_err(|_| EngineError {
                code: ErrorCode::BadInput,
            })?;
        let before = engine.libraries()?.libraries;
        explain(engine.dispatch(Method::Libraries, &params, &cancel), |code| {
            human::file_failure(path, code)
        })?;
        // Library ids are digests, never locations; people see the path they gave.
        for library in engine.libraries()?.libraries {
            if !before.iter().any(|old| old.id == library.id) {
                names.insert(library.id.as_str().to_owned(), path.display().to_string());
            }
        }
    }
    if matches.get_flag("zotero") {
        explain(
            engine.dispatch(Method::Libraries, &json!({"action":"open-zotero"}), &cancel),
            human::zotero_failure,
        )?;
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
    let libraries_open = || engine.libraries().is_ok_and(|result| !result.libraries.is_empty());
    let params = match method {
        Method::Libraries => json!({"action":"list"}),
        Method::Search => {
            let query = args.get_one::<String>("query").map_or("", String::as_str);
            let limit = args.get_one::<usize>("limit").copied().unwrap_or(10);
            let result = explain(
                engine.dispatch(Method::Search, &json!({"query":query,"limit":limit}), &cancel),
                |code| match code {
                    ErrorCode::BadInput if limit > 1000 => "--limit must be 1000 or less.".into(),
                    ErrorCode::BadInput => "the search words are too long; use fewer.".into(),
                    code => human::code_sentence(code).into(),
                },
            )?;
            return Ok(Execution::Result(
                human::with_summary(result, method, matches.get_flag("json"), libraries_open(), &names),
                None,
            ));
        }
        Method::Show => {
            let (params, key) = match args.get_one::<String>("id") {
                Some(id) => (
                    json!({"item":{"by":"id","value":explain(parse_json(id.as_bytes()), |_| ITEM_REF_HELP.into())?}}),
                    None,
                ),
                None => {
                    let key = args.get_one::<String>("key").cloned().unwrap_or_default();
                    (json!({"item":{"by":"key","value":key}}), Some(key))
                }
            };
            let result = explain(engine.dispatch(method, &params, &cancel), |code| match &key {
                Some(key) => human::key_failure(key, code, libraries_open()),
                None if code == ErrorCode::NotFound => "no open library holds the reference that --id names.".into(),
                None if code == ErrorCode::BadInput => ITEM_REF_HELP.into(),
                None => human::code_sentence(code).into(),
            })?;
            return Ok(Execution::Result(result, None));
        }
        Method::Cite if args.get_flag("request") => {
            const REQUEST_HELP: &str = "stdin must hold one CitationRequest JSON object; see: ibis schema cite";
            let params = explain(stdin_json(input), |_| REQUEST_HELP.into())?;
            let citing = request_citing(&params).ok_or_else(|| Failure::new(ErrorCode::BadInput, REQUEST_HELP))?;
            let result = explain(engine.dispatch(method, &params, &cancel), |code| match code {
                ErrorCode::BadInput => REQUEST_HELP.into(),
                code => human::code_sentence(code).into(),
            })?;
            return Ok(Execution::Result(result, Some(citing)));
        }
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
            if keys.len() > ibis_core::syntax::MAX_CITATION_ITEMS {
                return Err(Failure::new(
                    ErrorCode::BadInput,
                    format!(
                        "one citation can hold at most {} keys.",
                        ibis_core::syntax::MAX_CITATION_ITEMS
                    ),
                ));
            }
            let citing = human::Citing {
                labels: keys.clone(),
                output: args
                    .get_one::<String>("style")
                    .or_else(|| args.get_one::<String>("format"))
                    .cloned()
                    .unwrap_or_else(|| "pandoc".into()),
                form,
            };
            let result = engine
                .cite_keys(
                    &KeyCitationRequest {
                        keys: keys.clone(),
                        output,
                        form,
                        output_locale: args.get_one::<String>("locale").cloned(),
                    },
                    &cancel,
                )
                .map_err(|error| {
                    // The engine reports only a code; find the key that caused it.
                    let culprit = keys.iter().find_map(|key| {
                        engine
                            .dispatch(Method::Show, &json!({"item":{"by":"key","value":key}}), &cancel)
                            .err()
                            .map(|failure| (key, failure.code))
                    });
                    match culprit {
                        Some((key, code)) if code == error.code => {
                            Failure::new(code, human::key_failure(key, code, libraries_open()))
                        }
                        _ => Failure::new(error.code, human::code_sentence(error.code)),
                    }
                })?;
            let result = engine.finalize(DispatchResult::citation(result, keys.len())?)?;
            return Ok(Execution::Result(result, Some(citing)));
        }
        Method::Styles => json!({}),
        Method::Schema => {
            json!({"method":args.get_one::<String>("method").map(|name|method_named(name)).transpose()?})
        }
        Method::Call => {
            let called = method_named(args.get_one::<String>("method").map_or("", String::as_str))?;
            let params_help = || {
                format!(
                    "--params must be the JSON parameters of {}; see: ibis schema {}",
                    called.as_str(),
                    called.as_str()
                )
            };
            let params = if args.get_flag("stdin") {
                explain(stdin_json(input), |_| params_help())?
            } else {
                args.get_one::<String>("params")
                    .map(|params| explain(parse_json(params.as_bytes()), |_| params_help()))
                    .transpose()?
                    .unwrap_or(Value::Null)
            };
            let result = explain(
                engine.dispatch(method, &json!({"method":called,"params":params}), &cancel),
                |code| match code {
                    ErrorCode::BadInput => params_help(),
                    code => human::code_sentence(code).into(),
                },
            )?;
            let citing = (called == Method::Cite).then(|| request_citing(&params)).flatten();
            return Ok(Execution::Result(result, citing));
        }
    };
    let result = engine.dispatch(method, &params, &cancel)?;
    Ok(Execution::Result(
        human::with_summary(result, method, matches.get_flag("json"), libraries_open(), &names),
        None,
    ))
}
/// What a canonical CitationRequest asked for, so its findings read as sentences.
fn request_citing(params: &Value) -> Option<human::Citing> {
    let request: CitationRequest = serde_json::from_value(params.clone()).ok()?;
    Some(human::Citing {
        labels: vec![],
        output: match &request.output {
            OutputSpec::Csl { style } => style.clone(),
            OutputSpec::Key { syntax, .. } => serde_json::to_value(syntax)
                .ok()
                .and_then(|syntax| syntax.as_str().map(str::to_owned))
                .unwrap_or_default(),
            OutputSpec::Preset { .. } => "this output".into(),
        },
        form: request.form,
    })
}
const ITEM_REF_HELP: &str = "--id must be an ItemRef JSON object, as ibis search --json prints it.";
fn method_named(name: &str) -> Result<Method, Failure> {
    Method::parse(name).map_err(|code| {
        let names = Method::ALL.map(Method::as_str).join(", ");
        Failure::new(
            code,
            format!("there is no method '{}'; choose one of: {names}", human::field(name)),
        )
    })
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
fn emit(
    json: bool,
    result: Result<DispatchResult, Failure>,
    citing: Option<&human::Citing>,
    output: &mut impl Write,
    errors: &mut impl Write,
    mode: OutputMode,
) -> u8 {
    let mut say = |line: &str| {
        let _ = writeln!(errors, "ibis: {}", terminal_display(line));
    };
    if result.as_ref().is_ok_and(|result| result.source_findings) {
        say(if json {
            "note: a library has warnings or could not refresh; see: ibis libraries --json"
        } else {
            "note: a library has warnings or could not refresh; see: ibis libraries"
        });
    }
    if !json && let (Ok(result), Some(citing)) = (&result, citing) {
        for line in human::citation_report(&result.data, citing, result.text.is_some()) {
            say(&line);
        }
        if result.text.is_none() {
            // Refused citations print their reasons above and nothing to stdout.
            return result.exit_code.value();
        }
    }
    let (code, envelope, text) = match result.and_then(|result| result.bounded().map_err(Failure::from)) {
        Ok(result) => (result.exit_code, Envelope::success(result.data), result.text),
        Err(failure) => {
            if !json {
                say(failure
                    .message
                    .as_deref()
                    .unwrap_or_else(|| human::code_sentence(failure.code)));
                return failure.code.exit_code().value();
            }
            (failure.code.exit_code(), Envelope::failure(failure.code), None)
        }
    };
    let encoded = if json {
        serde_json::to_value(&envelope)
            .ok()
            .and_then(|value| ibis_engine::wire::encode(&value, protocol::MAX_RESPONSE_BYTES).ok())
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
            emit(true, Ok(result), None, &mut Broken, &mut Vec::new(), OutputMode::Plain),
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
