//! The CLI parses arguments and presents typed engine results; no source or citation rules live here.
use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::*;
use ibis_engine::{Cancellation, DispatchResult, Engine, Method, protocol, request::*};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};
mod app_libraries;
mod completions;
mod doctor;
mod human;
mod toggle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Terminal,
    Plain,
}

const LIBRARIES: &str = "Libraries";
const OUTPUT: &str = "Output";
const PER_KEY: &str = "Per key";
const ROOT_HELP: &str = "\
Examples:
  ibis search \"garcia 2021\" --file refs.bib
  ibis cite garcia2021 --file refs.bib --style apa
  ibis search memory --zotero --json

Add --json to any command for one versioned JSON envelope on stdout (serve and
mcp speak their own protocols instead). Messages go to stderr. Exit codes: 0 ok, 1 findings, 2 usage, 3 bad input, 4 not found,
5 source unavailable or resource limit, 6 newer format, 70 internal, 130 cancelled.

Reference for scripts and agents: https://github.com/ibiscite/ibis/blob/main/docs/cli.md";

fn command() -> Command {
    // Keep list values in their own scope: clap's global propagation replaces
    // root values when the same flag also appears after the subcommand.
    let file = Arg::new("file")
        .long("file")
        .value_name("PATH")
        .value_parser(clap::value_parser!(PathBuf))
        .action(ArgAction::Append)
        .help_heading(LIBRARIES)
        .display_order(1000)
        .help("Open a BibTeX, BibLaTeX or CSL-JSON file; repeat for several libraries");
    let group = Arg::new("zotero-group")
        .long("zotero-group")
        .value_name("GROUP_ID")
        .value_parser(clap::value_parser!(u64))
        .action(ArgAction::Append)
        .help_heading(LIBRARIES)
        .display_order(1002)
        .help("Read a local Zotero group library by its group ID; repeat for several");
    let mut root = Command::new("ibis")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Find references and cite them anywhere.")
        .after_help(ROOT_HELP)
        .subcommand_required(true)
        // Our own help command takes the global options, so `ibis help search --json` works.
        .disable_help_subcommand(true)
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Print the versioned JSON envelope, including errors and findings"),
        )
        .arg(file.clone());
    root = root.arg(
        Arg::new("zotero")
            .long("zotero")
            .global(true)
            .action(ArgAction::SetTrue)
            .help_heading(LIBRARIES)
            .display_order(1001)
            .help("Read the local Zotero personal library, open or closed"),
    );
    root = root.arg(group.clone()).arg(
        Arg::new("app-libraries")
            .long("app-libraries")
            .global(true)
            .action(ArgAction::SetTrue)
            .help_heading(LIBRARIES)
            .display_order(1003)
            .help(
                "Read the libraries chosen in the Ibis app, read-only; search, show, cite and libraries do this when no library is named",
            ),
    );
    for method in Method::ALL {
        let subcommand = Command::new(method.as_str());
        root = root.subcommand(match method {
            Method::Libraries => subcommand.about("List loaded libraries and source warnings"),
            Method::Search => subcommand
                .about("Search loaded records")
                .after_help("Examples:\n  ibis search \"garcia 2021\" --file refs.bib\n  ibis search memory --file refs.bib --json | jq '.data.hits[].item.object.data.csl.title'")
                .arg(
                    Arg::new("query")
                        .value_name("WORDS")
                        .required(true)
                        .help("Words from the author, title, year or citation key; quote several words"),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("N")
                        .value_parser(clap::value_parser!(usize))
                        .default_value("10")
                        .help("Return at most N results, up to 1000"),
                ),
            Method::Show => subcommand
                .about("Show one object by citation key or ItemRef")
                .arg(
                    Arg::new("key")
                        .value_name("KEY")
                        .required_unless_present("id")
                        .conflicts_with("id")
                        .help("The citation key of the reference"),
                )
                .arg(
                    Arg::new("id")
                        .long("id")
                        .value_name("ITEM_REF_JSON")
                        .help("An ItemRef JSON object, as ibis search --json prints it"),
                ),
            Method::Cite => subcommand
                .about("Render a citation through the shared engine")
                .after_help(
                    "Options after a key apply to that key, as in the picker:\n  \
                     ibis cite smith2020 --locator \"p. 12\" --prefix see jones2019 --omit-author\n\n\
                     Examples:\n  \
                     ibis cite smith2020 --file refs.bib --format autocite\n  \
                     ibis cite smith2020 --file refs.bib --style apa --bibliography\n  \
                     ibis schema cite   # the CitationRequest that --request reads",
                )
                .arg(
                    Arg::new("keys")
                        .value_name("KEY")
                        .help("Citation keys, in the order they appear in the citation")
                        .num_args(1..)
                        .required_unless_present("request")
                        .conflicts_with("request"),
                )
                .arg(
                    Arg::new("request")
                        .long("request")
                        .action(ArgAction::SetTrue)
                        .conflicts_with_all([
                            "keys",
                            "format",
                            "style",
                            "form",
                            "locale",
                            "bibliography",
                            "locator",
                            "prefix",
                            "suffix",
                            "omit-author",
                        ])
                        .help("Read a canonical CitationRequest JSON object from stdin"),
                )
                .arg(Arg::new("format").long("format").value_name("FORMAT").help_heading(OUTPUT).help("Key syntax to write [default: pandoc]").conflicts_with("style").value_parser([
                    "pandoc",
                    "autocite",
                    "parencite",
                    "textcite",
                    "cite",
                    "typst",
                    "keys",
                    "csl-json",
                    "natbib",
                    "org-cite",
                    "myst",
                    "multi-markdown",
                    "ascii-doc",
                    "jupyter",
                    "jekyll-scholar",
                    "cayw-json",
                ]))
                .arg(
                    Arg::new("style")
                        .long("style")
                        .value_name("STYLE_ID")
                        .help_heading(OUTPUT)
                        .help("Format with a CSL style instead, such as apa; list them with: ibis styles"),
                )
                .arg(
                    Arg::new("bibliography")
                        .long("bibliography")
                        .action(ArgAction::SetTrue)
                        .requires("style")
                        .conflicts_with("form")
                        .help_heading(OUTPUT)
                        .help("Format bibliography entries using --style"),
                )
                .arg(Arg::new("form").long("form").value_name("FORM").help_heading(OUTPUT).help("Citation form [default: normal]").value_parser([
                    "normal",
                    "narrative",
                    "author-only",
                    "year-only",
                    "full",
                    "bibliography-only",
                ]))
                .arg(
                    Arg::new("locale")
                        .long("locale")
                        .value_name("LOCALE")
                        .help_heading(OUTPUT)
                        .help("Language for --style output, such as en-GB or de-DE"),
                )
                .arg(
                    Arg::new("locator")
                        .long("locator")
                        .help_heading(PER_KEY)
                        .value_name("LOCATOR")
                        .action(ArgAction::Append)
                        .help("Pinpoint the key before it, such as \"p. 12\", \"ch. 3\" or \"12-14\""),
                )
                .arg(
                    Arg::new("prefix")
                        .long("prefix")
                        .help_heading(PER_KEY)
                        .value_name("TEXT")
                        .action(ArgAction::Append)
                        .help("Text before the key before it, such as \"see\""),
                )
                .arg(
                    Arg::new("suffix")
                        .long("suffix")
                        .help_heading(PER_KEY)
                        .value_name("TEXT")
                        .action(ArgAction::Append)
                        .help("Text after the key before it"),
                )
                .arg(
                    Arg::new("omit-author")
                        .long("omit-author")
                        .help_heading(PER_KEY)
                        // Append keeps every occurrence's index; Count keeps only the last.
                        .num_args(0)
                        .default_missing_value("true")
                        .action(ArgAction::Append)
                        .help("Leave out the author of the key before it"),
                ),
            Method::Styles => subcommand.about("List available bundled CSL styles"),
            Method::Schema => subcommand
                .about("Print generated method JSON schemas")
                .arg(
                    Arg::new("method")
                        .value_name("METHOD")
                        .help("Print only this method's schema; leave out for every method"),
                ),
            Method::Call => subcommand
                .about("Call any typed engine method")
                .after_help("Example:\n  ibis call search --params '{\"query\":\"garcia\",\"limit\":5}' --file refs.bib --json")
                .arg(
                    Arg::new("method")
                        .value_name("METHOD")
                        .required(true)
                        .help("The engine method, as ibis schema lists them"),
                )
                .arg(
                    Arg::new("params")
                        .long("params")
                        .value_name("JSON")
                        .conflicts_with("stdin")
                        .help("The method's parameters as one JSON object"),
                )
                .arg(
                    Arg::new("stdin")
                        .long("stdin")
                        .action(ArgAction::SetTrue)
                        .help("Read method parameters from stdin"),
                ),
        });
    }
    root = root
        .subcommand(
            Command::new("serve")
                .about("Serve JSON-RPC over Content-Length framed stdio")
                .arg(
                    Arg::new("stdio")
                        .long("stdio")
                        .required(true)
                        .action(ArgAction::SetTrue)
                        .help("Use stdin and stdout, the only transport"),
                ),
        )
        .subcommand(
            Command::new("mcp").about("Serve read-only search and citation tools to AI agents (MCP over stdio)"),
        )
        .subcommand(
            Command::new("toggle").about("Show or hide the desktop picker").arg(
                Arg::new("app")
                    .long("app")
                    .value_name("PATH")
                    .value_parser(clap::value_parser!(PathBuf))
                    .help("The Ibis app to start when it is not running"),
            ),
        )
        .subcommand(
            Command::new("doctor")
                .about("Report source health offline, without writing or sending anything")
                .arg(
                    Arg::new("config")
                        .long("config")
                        .value_name("PATH")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Check a chosen Ibis settings file without repairing it or loading its libraries"),
                ),
        )
        .subcommand(
            Command::new("completions")
                .about("Generate shell completions from the CLI commands")
                .arg(
                    Arg::new("shell")
                        .value_name("SHELL")
                        .required(true)
                        .value_parser(["bash", "zsh", "fish"])
                        .help("The shell to generate completions for"),
                ),
        );
    // The command list comes from the tree itself, so completions offer every help target.
    let mut targets: Vec<String> = root
        .get_subcommands()
        .map(|command| command.get_name().to_owned())
        .collect();
    targets.push("help".to_owned());
    root.subcommand(
        Command::new("help")
            .about("Print this message or the help of the given command")
            .arg(
                Arg::new("command")
                    .value_name("COMMAND")
                    .value_parser(clap::builder::PossibleValuesParser::new(targets))
                    .hide_possible_values(true)
                    .help("The command to explain, such as cite"),
            ),
    )
    .mut_subcommands(|subcommand| subcommand.arg(file.clone()).arg(group.clone()))
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
        Ok(engine) => run_engine(engine, arguments, input, output, errors, mode, Defaults::App),
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
/// The same parser and dispatcher can run over a preloaded engine without reading user
/// files: with no library named, it uses the engine as given, never the app's choice.
pub fn run_with<R: BufRead, W: Write + Send, E: Write>(
    engine: Engine,
    arguments: Vec<std::ffi::OsString>,
    input: R,
    output: W,
    errors: E,
    mode: OutputMode,
) -> u8 {
    run_engine(engine, arguments, input, output, errors, mode, Defaults::None)
}

/// What a command that reads libraries uses when none is named.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Defaults {
    /// The libraries chosen in the Ibis app.
    App,
    /// Only what the engine already holds.
    None,
}

fn run_engine<R: BufRead, W: Write + Send, E: Write>(
    engine: Engine,
    arguments: Vec<std::ffi::OsString>,
    mut input: R,
    mut output: W,
    mut errors: E,
    mode: OutputMode,
    defaults: Defaults,
) -> u8 {
    let json = arguments.iter().any(|argument| argument == "--json");
    let program = program_name(arguments.first());
    let matches = match command().bin_name(&program).try_get_matches_from(arguments.iter()) {
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
                            data: if error.kind() == clap::error::ErrorKind::DisplayVersion {
                                json!({"version":env!("CARGO_PKG_VERSION")})
                            } else {
                                json!({"help":error.to_string()})
                            },
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
            // clap names the argument and prints the usage line; keep its words,
            // but a line break typed in an argument must not start a forged line.
            let mut message = error.to_string();
            // clap may quote a whole argument or only the value after `--flag=`.
            for argument in &arguments {
                let text = argument.to_string_lossy();
                let value = text.split_once('=').map(|(_, value)| value);
                for fragment in std::iter::once(text.as_ref()).chain(value) {
                    if fragment.contains(['\n', '\r']) {
                        message = message.replace(fragment, &fragment.escape_debug().to_string());
                    }
                }
            }
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
    if let Some(("help", args)) = matches.subcommand() {
        let help = command_help(&program, args.get_one::<String>("command"));
        return emit(json, Ok(help), None, &mut output, &mut errors, mode);
    }
    match execute(&engine, &matches, defaults, &mut input, &mut output, mode) {
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

/// The name this program was started as, such as `ibis.exe`; `ibis` when that name is
/// empty or not text, so parsing and help always show the same one.
fn program_name(program: Option<&std::ffi::OsString>) -> String {
    program
        .and_then(|program| Path::new(program).file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("ibis")
        .to_owned()
}

/// The help `ibis <command> --help` prints.
fn command_help(program: &str, target: Option<&String>) -> DispatchResult {
    let mut root = command().bin_name(program);
    root.build();
    // The parser only accepts names from the tree, so an unknown target cannot get here.
    let mut shown = target
        .and_then(|target| root.find_subcommand(target).cloned())
        .unwrap_or(root);
    let help = shown.render_help().to_string();
    DispatchResult {
        text: Some(help.trim_end().to_owned()),
        data: json!({"help":help}),
        source_findings: false,
        exit_code: ibis_core::contract::ExitCode::Ok,
    }
}

enum Execution {
    Result(DispatchResult, Option<human::Citing>),
    Stdio(u8),
}

fn source_values<T: Clone + Eq + Send + Sync + 'static>(matches: &ArgMatches, id: &str) -> Vec<T> {
    let mut values = Vec::new();
    for scope in std::iter::once(matches).chain(matches.subcommand().map(|(_, args)| args)) {
        for value in scope.get_many::<T>(id).into_iter().flatten() {
            if !values.contains(value) {
                values.push(value.clone());
            }
        }
    }
    values
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
    defaults: Defaults,
    input: &mut R,
    output: &mut W,
    mode: OutputMode,
) -> Result<Execution, Failure> {
    let (name, args) = matches.subcommand().ok_or(EngineError { code: ErrorCode::Usage })?;
    if name == "doctor" {
        return Ok(Execution::Result(doctor::run(engine, matches, mode)?, None));
    }
    if name == "completions" {
        let shell = args
            .get_one::<String>("shell")
            .ok_or(EngineError { code: ErrorCode::Usage })?;
        let script = completions::generate(command(), shell);
        return Ok(Execution::Result(
            DispatchResult {
                data: json!({"shell":shell, "script":script}),
                text: Some(script),
                source_findings: false,
                exit_code: ibis_core::contract::ExitCode::Ok,
            },
            None,
        ));
    }
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
    let mut files = source_values::<PathBuf>(matches, "file");
    let mut groups = source_values::<u64>(matches, "zotero-group");
    let mut personal = matches.get_flag("zotero");
    let named = !files.is_empty() || !groups.is_empty() || personal;
    // Named libraries replace the app's; servers read the app's only when asked. A
    // preloaded engine never reads the app's settings, and says so if asked to.
    let asked = matches.get_flag("app-libraries");
    if asked && defaults == Defaults::None {
        return Err(Failure::new(
            ErrorCode::Usage,
            "--app-libraries is not available here; name the libraries to read.",
        ));
    }
    let app =
        if asked || (defaults == Defaults::App && !named && matches!(name, "search" | "show" | "cite" | "libraries")) {
            Some(app_libraries::load()?)
        } else {
            None
        };
    let named_files = files.clone();
    let (named_personal, named_groups) = (personal, groups.clone());
    if let Some(app) = &app {
        for path in &app.files {
            if !files.contains(path) {
                files.push(path.clone());
            }
        }
        for library in &app.zotero {
            match library {
                ZoteroLibrary::Personal => personal = true,
                ZoteroLibrary::Group(id) if !groups.contains(id) => groups.push(*id),
                ZoteroLibrary::Group(_) => {}
            }
        }
    }
    // A path named on the command line is the person's own, even if the app chose it too.
    let chosen_in = |path: &PathBuf| app.as_ref().filter(|_| !named_files.contains(path)).map(|app| app.app);
    for path in files {
        let params =
            serde_json::to_value(LibrariesRequest::OpenFile { path: path.clone() }).map_err(|_| EngineError {
                code: ErrorCode::BadInput,
            })?;
        let before = engine.libraries()?.libraries;
        let chosen = chosen_in(&path);
        // A path the app chose is never shown: agents read these messages too.
        explain(
            engine.dispatch(Method::Libraries, &params, &cancel),
            |code| match chosen {
                Some(app) => format!(
                    "cannot open the library chosen in {app}: {} Choose it again there, or {}.",
                    human::file_reason(&path, code),
                    app_libraries::NAME_ONE
                ),
                None => human::file_failure(&path, code),
            },
        )?;
        // Library ids are digests, never locations; people see the path they gave.
        for library in engine.libraries()?.libraries {
            if !before.iter().any(|old| old.id == library.id) {
                let shown = match chosen {
                    Some(app) => format!("Library chosen in {app}"),
                    None => path.display().to_string(),
                };
                names.insert(library.id.as_str().to_owned(), shown);
            }
        }
    }
    let zotero: Vec<_> = personal
        .then_some(ZoteroLibrary::Personal)
        .into_iter()
        .chain(groups.into_iter().map(ZoteroLibrary::Group))
        .collect();
    if !zotero.is_empty() {
        let params = serde_json::to_value(LibrariesRequest::OpenZotero {
            libraries: Some(zotero),
        })
        .map_err(|_| EngineError {
            code: ErrorCode::BadInput,
        })?;
        let before = engine.libraries()?.libraries;
        explain(
            engine.dispatch(Method::Libraries, &params, &cancel),
            human::zotero_failure,
        )?;
        // Each Zotero library is the app's only when the app chose it and the command line did not.
        if let Some(app) = &app {
            for library in engine.libraries()?.libraries {
                if before.iter().any(|old| old.id == library.id) {
                    continue;
                }
                let group = serde_json::to_value(&library)
                    .ok()
                    .and_then(|value| value.pointer("/zotero/group").and_then(Value::as_u64));
                let (chosen, shown) = match group {
                    Some(group) => (
                        app.zotero.contains(&ZoteroLibrary::Group(group)) && !named_groups.contains(&group),
                        format!("Zotero group {group} (chosen in {})", app.app),
                    ),
                    None => (
                        app.zotero.contains(&ZoteroLibrary::Personal) && !named_personal,
                        format!("Zotero (chosen in {})", app.app),
                    ),
                };
                if chosen {
                    names.insert(library.id.as_str().to_owned(), shown);
                }
            }
        }
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
                    other => (
                        serde_json::from_value(json!(other)).map_err(|_| EngineError { code: ErrorCode::Usage })?,
                        None,
                    ),
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
            let keys = key_items(args)?;
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
                labels: keys.iter().map(|item| item.key.clone()).collect(),
                flags: keys
                    .iter()
                    .map(|item| {
                        [
                            (item.locator.is_some(), "--locator"),
                            (!item.prefix.is_empty(), "--prefix"),
                            (!item.suffix.is_empty(), "--suffix"),
                            (item.suppress_author, "--omit-author"),
                        ]
                        .into_iter()
                        .filter_map(|(given, flag)| given.then_some(flag))
                        .collect()
                    })
                    .collect(),
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
                    let culprit = keys.iter().map(|item| &item.key).find_map(|key| {
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
        flags: vec![],
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
/// Keys with the options that follow each one, the way the picker attaches them.
fn key_items(args: &ArgMatches) -> Result<Vec<KeyItem>, Failure> {
    let mut items: Vec<(usize, KeyItem)> = args
        .get_many::<String>("keys")
        .into_iter()
        .flatten()
        .zip(args.indices_of("keys").into_iter().flatten())
        .map(|(key, index)| (index, KeyItem::from(key.as_str())))
        .collect();
    let owner = |flag: &str, index: usize| {
        items
            .iter()
            .rposition(|(start, _)| *start < index)
            .ok_or_else(|| Failure::new(ErrorCode::Usage, format!("--{flag} must follow the key it belongs to.")))
    };
    let mut options: Vec<(usize, &str, Option<&String>)> = vec![];
    for flag in ["locator", "prefix", "suffix"] {
        let values = args.get_many::<String>(flag).into_iter().flatten();
        let indices = args.indices_of(flag).into_iter().flatten();
        options.extend(indices.zip(values).map(|(index, value)| (index, flag, Some(value))));
    }
    options.extend(
        args.indices_of("omit-author")
            .into_iter()
            .flatten()
            .map(|index| (index, "omit-author", None)),
    );
    let mut resolved = vec![];
    for (index, flag, value) in options {
        resolved.push((owner(flag, index)?, flag, value));
    }
    for (position, flag, value) in resolved {
        let item = &mut items[position].1;
        let text = || {
            vec![TextRun {
                text: value.cloned().unwrap_or_default(),
                italic: false,
                bold: false,
                script: Script::Normal,
            }]
        };
        let repeated = match flag {
            "locator" => item
                .locator
                .replace(locator(value.map_or("", String::as_str))?)
                .is_some(),
            "prefix" => !std::mem::replace(&mut item.prefix, text()).is_empty(),
            "suffix" => !std::mem::replace(&mut item.suffix, text()).is_empty(),
            _ => std::mem::replace(&mut item.suppress_author, true),
        };
        if repeated {
            return Err(Failure::new(
                ErrorCode::Usage,
                format!("--{flag} is given twice for '{}'.", human::field(&item.key)),
            ));
        }
    }
    Ok(items.into_iter().map(|(_, item)| item).collect())
}
/// The canonical locator reader, applied to the whole value.
fn locator(text: &str) -> Result<Locator, Failure> {
    [format!("x {text}"), format!("x, {text}")]
        .iter()
        .filter_map(|query| ibis_core::locator::parse_trailing(query))
        .find(|split| split.search == "x")
        .map(|split| split.locator)
        .ok_or_else(|| {
            Failure::new(
                ErrorCode::Usage,
                format!(
                    "--locator '{}' is not a locator Ibis reads; write it like \"p. 12\", \"ch. 3\" or \"12-14\".",
                    human::field(text)
                ),
            )
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
    // Continuation lines are indented, so no text inside a message can pass
    // for a separate `ibis:` diagnostic.
    let mut say = |line: &str| {
        let text = terminal_display(line);
        let mut lines = text.split('\n');
        let _ = writeln!(errors, "ibis: {}", lines.next().unwrap_or_default());
        for line in lines {
            let _ = if line.is_empty() {
                writeln!(errors)
            } else {
                writeln!(errors, "  {line}")
            };
        }
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
            // The sentence goes to stderr in both modes; --json adds the stable code on stdout.
            say(failure
                .message
                .as_deref()
                .unwrap_or_else(|| human::code_sentence(failure.code)));
            if !json {
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
    fn source_flags_preserve_order_across_positions_and_ignore_duplicates() -> Result<(), clap::Error> {
        let matches = command().try_get_matches_from([
            "ibis",
            "--zotero-group=9",
            "--file=first.bib",
            "--zotero-group",
            "7",
            "--file",
            "second.bib",
            "search",
            "x",
            "--zotero-group",
            "11",
            "--file",
            "third.bib",
            "--zotero-group",
            "9",
            "--file=first.bib",
        ])?;
        assert_eq!(source_values::<u64>(&matches, "zotero-group"), [9, 7, 11]);
        assert_eq!(
            source_values::<PathBuf>(&matches, "file"),
            ["first.bib", "second.bib", "third.bib"].map(PathBuf::from)
        );
        Ok(())
    }

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
    fn every_argument_and_option_explains_itself_in_help() {
        fn check(command: &Command, path: &str) {
            for argument in command.get_arguments() {
                assert!(
                    argument.get_help().is_some(),
                    "{path} {} has no help text",
                    argument.get_id()
                );
            }
            for subcommand in command.get_subcommands() {
                check(subcommand, &format!("{path} {}", subcommand.get_name()));
            }
        }
        let mut root = command();
        root.build();
        check(&root, "ibis");
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
