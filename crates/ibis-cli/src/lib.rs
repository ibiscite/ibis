//! The CLI parses arguments and presents typed engine results; no source or citation rules live here.
use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::*;
use ibis_core::destination::{System, destination};
use ibis_engine::{Cancellation, DispatchResult, Engine, Method, protocol, request::*};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};
mod app_libraries;
mod completions;
mod doctor;
#[cfg(test)]
mod generated;
mod human;
mod settings_backup;
mod toggle;
mod writing_projects;

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
  ibis search lantern --file refs.bib
  ibis cite vale2024lanterns --file refs.bib --style apa
  ibis search lantern --zotero --json

Add --json to any command for one versioned JSON envelope on stdout (serve and
mcp speak their own protocols instead). Messages go to stderr. Exit codes: 0 ok, 1 findings, 2 usage, 3 bad input, 4 not found,
5 source unavailable or resource limit, 6 newer format, 70 internal, 130 cancelled.

Reference for scripts and agents: https://github.com/ibiscite/ibis/blob/main/docs/cli.md";
const GREETING: &str = "\
finds references and cites them anywhere.

Try it:
  ibis search lantern --file refs.bib       find a reference
  ibis cite vale2024lanterns --file refs.bib --style apa
                                            make a citation
  ibis doctor --file refs.bib               check a library

With the Ibis app installed you can leave out --file.
Add --json to any command for scripts and agents. All commands: ibis --help";

/// A number of search results a person can ask for.
fn search_limit(text: &str) -> Result<usize, String> {
    text.parse()
        .ok()
        .filter(|limit| (1..=1000).contains(limit))
        .ok_or_else(|| "give a whole number from 1 to 1000".to_owned())
}

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
    let profile = Arg::new("zotero-profile")
        .long("zotero-profile")
        .value_name("NAME")
        .help_heading(LIBRARIES)
        .display_order(1003)
        .help("Which Zotero profile to read while Zotero is closed, when there are several");
    let mut root = Command::new("ibis")
        .version(ibis_core::VERSION)
        .about("Find references and cite them anywhere.")
        .after_help(ROOT_HELP)
        .subcommand_required(true)
        .arg_required_else_help(true)
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
    root = root.arg(group.clone()).arg(profile.clone()).arg(
        Arg::new("app-libraries")
            .long("app-libraries")
            .global(true)
            .action(ArgAction::SetTrue)
            .help_heading(LIBRARIES)
            .display_order(1004)
            .help(
                "Read the libraries chosen in the Ibis app, read-only; search, show, cite and libraries do this when no library is named",
            ),
    );
    for method in Method::ALL {
        let subcommand = Command::new(method.as_str());
        root = root.subcommand(match method {
            Method::Libraries => subcommand.about("List the libraries and their warnings"),
            Method::Search => subcommand
                .about("Find references")
                .after_help("Examples:\n  ibis search lantern --file refs.bib\n  ibis search lantern --file refs.bib --json | jq '.data.hits[].item.object.data.csl.title'")
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
                        .value_parser(search_limit)
                        .default_value("10")
                        .help("Return at most N results, from 1 to 1000"),
                ),
            Method::Show => subcommand
                .about("Show one reference")
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
                        .value_name("ID_JSON")
                        .help("The reference's id, as the JSON that ibis search --json prints"),
                ),
            Method::Cite => subcommand
                .about("Write a citation")
                .after_help(
                    "Options after a key apply to that key, as in the picker:\n  \
                     ibis cite vale2024lanterns --locator \"p. 12\" --prefix see fen2023clouds --omit-author\n\n\
                     Examples:\n  \
                     ibis cite vale2024lanterns --file refs.bib --format autocite\n  \
                     ibis cite vale2024lanterns --file refs.bib --style apa --bibliography\n  \
                     ibis cite vale2024lanterns --file refs.bib --for-app com.microsoft.Word\n  \
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
                            "for-app",
                            "project",
                            "form",
                            "locale",
                            "bibliography",
                            "locator",
                            "prefix",
                            "suffix",
                            "omit-author",
                        ])
                        .help("Read the whole request from stdin as JSON; see: ibis schema cite"),
                )
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_name("FORMAT")
                        .help_heading(OUTPUT)
                        .help(format!(
                            "Output to write: a key syntax or a common style [default: {}]; list them with: ibis outputs",
                            ibis_engine::outputs::DEFAULT
                        ))
                        .hide_possible_values(true)
                        .conflicts_with("style")
                        .value_parser(output_ids()),
                )
                .arg(
                    Arg::new("style")
                        .long("style")
                        .value_name("STYLE_ID")
                        .help_heading(OUTPUT)
                        .help("Format with any CSL style instead; list them with: ibis styles"),
                )
                .arg(
                    Arg::new("project").long("project").value_name("PROJECT_ID")
                        .help_heading(OUTPUT)
                        .conflicts_with_all(["format", "style", "for-app", "request"])
                        .help("Use this writing project's output and bibliography; list IDs with: ibis projects list"),
                )
                .arg(Arg::new("nightly").long("nightly").action(ArgAction::SetTrue)
                    .requires("project").help("Read the writing project from Ibis Nightly's settings"))
                .arg(Arg::new("strict-project").long("strict-project").action(ArgAction::SetTrue)
                    .requires("project").help("Require a current complete bibliography read for project key citations"))
                .arg(
                    Arg::new("for-app")
                        .long("for-app")
                        .value_name("APP_ID")
                        .value_parser(clap::builder::NonEmptyStringValueParser::new())
                        .help_heading(OUTPUT)
                        .help("Use the output Ibis uses for this app, a macOS bundle ID or Windows executable; --style and --format apply when it has none of its own"),
                )
                .arg(
                    Arg::new("bibliography")
                        .long("bibliography")
                        .action(ArgAction::SetTrue)
                        .conflicts_with("form")
                        .help_heading(OUTPUT)
                        .help("Format bibliography entries, in --style or a style --format names"),
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
                        .help("Language for --style output, such as de-DE; default the style's own. List them with: ibis styles --locales"),
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
            Method::Project => subcommand
                .about("Check citation keys against a project's own .bib, read-only")
                .after_help(
                    "Each exact key is reported as in the file, missing, repeated, doubtful or not checkable.\n\
                     Name a library too, and a key whose entry disagrees with it is flagged.\n\n\
                     Examples:\n  \
                     ibis project thesis.bib vale2024lanterns fen2023clouds\n  \
                     ibis project thesis.bib vale2024lanterns --file refs.bib --json",
                )
                .arg(
                    Arg::new("bib")
                        .value_name("BIB_FILE")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("The .bib the document compiles against; it is never changed"),
                )
                .arg(
                    Arg::new("keys")
                        .value_name("KEY")
                        .required(true)
                        .num_args(1..)
                        .help("Citation keys, exactly as the document uses them"),
                ),
            Method::Entry => subcommand
                .about("Print a reference's original BibTeX entry, with what it needs to stand alone")
                .after_help(
                    "The entry comes as written, key included, with its @string, crossref and xdata.\n\n\
                     Example:\n  ibis entry vale2024lanterns --file refs.bib >> thesis.bib",
                )
                .arg(Arg::new("key").value_name("KEY").required(true).help("The citation key of the reference")),
            Method::Styles => subcommand
                .about("Find bundled CSL styles and citation languages")
                .after_help("Examples:\n  ibis styles            # the common styles first\n  ibis styles nature\n  ibis styles \"gb/t 7714\"\n  ibis styles --locales")
                .arg(
                    Arg::new("query")
                        .value_name("WORDS")
                        .num_args(0..)
                        .help("Words from a style's title or id; every word must match"),
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .value_name("N")
                        .value_parser(clap::value_parser!(usize))
                        .default_value("100")
                        .help("Return at most N styles, up to 20000"),
                )
                .arg(
                    Arg::new("offset")
                        .long("offset")
                        .value_name("N")
                        .value_parser(clap::value_parser!(usize))
                        .default_value("0")
                        .help("Skip the first N styles"),
                )
                .arg(
                    Arg::new("locales")
                        .long("locales")
                        .action(ArgAction::SetTrue)
                        .help("List citation languages instead of styles"),
                ),
            Method::Outputs => subcommand
                .about("List the outputs cite writes: key syntaxes, then the common styles")
                .after_help("Each id is what --format takes. Any other bundled style: ibis styles"),
            Method::Schema => subcommand
                .about("Print the JSON Schema of each method")
                .arg(
                    Arg::new("method")
                        .value_name("METHOD")
                        .help("Print only this method's schema; leave out for every method"),
                ),
            Method::Call => subcommand
                .about("Call a method with JSON parameters")
                .after_help("Example:\n  ibis call search --params '{\"query\":\"garcia\",\"limit\":5}' --file refs.bib --json")
                .arg(
                    Arg::new("method")
                        .value_name("METHOD")
                        .required(true)
                        .help("The method, as ibis schema lists them"),
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
                .about("Serve JSON-RPC to other programs over stdio")
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
        .subcommand(settings_backup::command())
        .subcommand(writing_projects::command())
        .subcommand(
            Command::new("completions").about("Print shell completions").arg(
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
    .mut_subcommands(|subcommand| subcommand.arg(file.clone()).arg(group.clone()).arg(profile.clone()))
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
            "",
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
    // A person at a terminal gets a start; a script still gets the usage error.
    if arguments.len() < 2 && mode == OutputMode::Terminal {
        let greeting = format!("Ibis {} {GREETING}\n", ibis_core::VERSION);
        return if output.write_all(greeting.as_bytes()).is_ok() {
            0
        } else {
            70
        };
    }
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
                                json!({"version":ibis_core::VERSION})
                            } else {
                                json!({"help":error.to_string()})
                            },
                            text: None,
                            source_findings: false,
                            exit_code: ibis_core::contract::ExitCode::Ok,
                        }),
                        None,
                        "",
                        &mut output,
                        &mut errors,
                        mode,
                    );
                }
                return match write!(output, "{error}") {
                    Err(failure) if failure.kind() != std::io::ErrorKind::BrokenPipe => 70,
                    _ => 0,
                };
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
                "",
                &mut output,
                &mut errors,
                mode,
            );
        }
    };
    if defaults == Defaults::App {
        // Doctor writes nothing: without a saved key its ids last only for its report.
        ibis_engine::identity::use_saved_key(matches.subcommand_name() != Some("doctor"));
    }
    let engine = if matches!(matches.subcommand_name(), Some("serve" | "mcp")) {
        engine
    } else {
        engine.without_file_watches()
    };
    if let Some(("help", args)) = matches.subcommand() {
        let help = command_help(&program, args.get_one::<String>("command"));
        return emit(json, Ok(help), None, "", &mut output, &mut errors, mode);
    }
    let libraries = library_options(&matches);
    match execute(&engine, &matches, defaults, &mut input, &mut output, &mut errors, mode) {
        Ok(Execution::Stdio(code)) => code,
        Ok(Execution::Result(result, citing)) => emit(
            json,
            Ok(result),
            citing.as_ref(),
            &libraries,
            &mut output,
            &mut errors,
            mode,
        ),
        Err(failure) if matches.subcommand_name() == Some("serve") => {
            let error = failure.error;
            if protocol::write_frame(&mut output, &ibis_engine::rpc::failure(None, error)).is_err() {
                70
            } else {
                error.code.exit_code().value()
            }
        }
        Err(failure) => emit(json, Err(failure), None, "", &mut output, &mut errors, mode),
    }
}

/// The library options of this run, as they would be typed again.
fn library_options(matches: &ArgMatches) -> String {
    let mut options = String::new();
    for path in source_values::<PathBuf>(matches, "file") {
        options.push_str(&format!(" --file {}", human::quoted(&path.display().to_string())));
    }
    if matches.get_flag("zotero") {
        options.push_str(" --zotero");
    }
    for group in source_values::<u64>(matches, "zotero-group") {
        options.push_str(&format!(" --zotero-group {group}"));
    }
    if matches.get_flag("app-libraries") {
        options.push_str(" --app-libraries");
    }
    options
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

/// Names for the refusal, read only once a read has been refused.
fn zotero_profiles(engine: &Engine, cancel: &Cancellation) -> Vec<String> {
    let request = json!({"action":"zotero-profiles"});
    engine
        .dispatch(Method::Libraries, &request, cancel)
        .ok()
        .and_then(|result| serde_json::from_value(result.data.get("zotero_profiles")?.clone()).ok())
        .unwrap_or_default()
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
/// Runs a filesystem wait on its own thread for at most the two seconds that
/// docs/architecture.md allows, so a stalled disk cannot hold a command. A
/// one-shot command leaves at most one such call behind before it exits.
pub(crate) fn bounded<T: Send + 'static>(operation: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("ibis-bounded-io".into())
        .spawn(move || {
            let _ = sender.send(operation());
        })
        .ok()?;
    receiver.recv_timeout(std::time::Duration::from_secs(2)).ok()
}

struct Failure {
    error: EngineError,
    message: Option<String>,
    /// The sentence for readers who must not see the file it names, when `message` names one.
    unnamed: Option<String>,
}
impl Failure {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::explained(EngineError::new(code), message)
    }
    fn explained(error: EngineError, message: impl Into<String>) -> Self {
        Self {
            error,
            message: Some(message.into()),
            unnamed: None,
        }
    }
    fn sentence(&self) -> &str {
        self.message
            .as_deref()
            .unwrap_or_else(|| human::code_sentence(self.error.code))
    }
    /// What an agent reads when its libraries did not open. Agents never see paths.
    fn refusal(&self) -> ibis_engine::mcp::Refusal {
        let reason = self.unnamed.as_deref().unwrap_or_else(|| self.sentence());
        ibis_engine::mcp::Refusal::new(
            self.error.code,
            format!(
                "The library could not be opened: {reason} Pass this on to the user; once it is fixed, start a new session."
            ),
        )
    }
}
impl From<EngineError> for Failure {
    fn from(error: EngineError) -> Self {
        Self {
            error,
            message: error.reason.map(|reason| human::reason_sentence(reason).to_owned()),
            unnamed: None,
        }
    }
}
/// Why Zotero did not open. Profile names are the person's own choosing, so
/// an agent reads the sentence without them, as it reads none of their paths.
fn zotero_open_failure(error: EngineError, profiles: &[String]) -> Failure {
    let mut failure = Failure::explained(error, human::zotero_failure(error, profiles));
    failure.unnamed = Some(human::zotero_failure(error, &[]));
    failure
}
/// Why a citation key selected no single reference, with every line that repeats it.
fn key_failure(engine: &Engine, key: &str, error: EngineError, libraries_open: bool) -> Failure {
    let lines: Vec<usize> = if error.reason == Some(ErrorReason::DuplicateKey) {
        engine
            .libraries()
            .map(|result| result.libraries)
            .unwrap_or_default()
            .iter()
            .flat_map(|library| &library.warnings)
            .filter(|warning| warning.key.as_deref() == Some(key))
            .filter_map(|warning| warning.line)
            .collect()
    } else {
        vec![]
    };
    Failure::explained(error, human::key_failure(key, error, libraries_open, &lines))
}
/// Attach a sentence to an engine error without changing its code or reason.
/// A reason has its own sentence; front ends never guess one from the code.
fn explain<T>(result: Result<T, EngineError>, message: impl FnOnce(EngineError) -> String) -> Result<T, Failure> {
    result.map_err(|error| match error.reason {
        Some(reason) => Failure::explained(error, human::reason_sentence(reason)),
        None => Failure::explained(error, message(error)),
    })
}

fn execute<R: BufRead, W: Write + Send, E: Write>(
    engine: &Engine,
    matches: &ArgMatches,
    defaults: Defaults,
    input: &mut R,
    output: &mut W,
    errors: &mut E,
    mode: OutputMode,
) -> Result<Execution, Failure> {
    let (name, args) = matches.subcommand().ok_or(EngineError::new(ErrorCode::Usage))?;
    if name == "doctor" {
        return Ok(Execution::Result(doctor::run(engine, matches, mode)?, None));
    }
    if name == "settings" {
        return Ok(Execution::Result(settings_backup::run(args)?, None));
    }
    if name == "projects" {
        return Ok(Execution::Result(writing_projects::run(engine, args)?, None));
    }
    if name == "completions" {
        let shell = args
            .get_one::<String>("shell")
            .ok_or(EngineError::new(ErrorCode::Usage))?;
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
            |_| "could not prepare the toggle; check that the temporary folder can be written, then try again.".into(),
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
    let opened = open_libraries(engine, matches, name, defaults);
    if name == "mcp" {
        let session = match opened {
            Ok(_) => ibis_engine::mcp::Session::default(),
            Err(failure) => {
                let _ = writeln!(errors, "ibis: {}", terminal_display(failure.sentence()));
                ibis_engine::mcp::Session::without_libraries(failure.refusal())
            }
        };
        return Ok(Execution::Stdio(
            match ibis_engine::mcp::serve(engine, session, input, output) {
                Ok(()) => 0,
                Err(_) => 70,
            },
        ));
    }
    let names = opened?;
    if name == "serve" {
        return Ok(Execution::Stdio(
            match ibis_engine::server::serve(engine.clone(), input, output) {
                Ok(()) => 0,
                Err(protocol::FrameError::Io) => 70,
                Err(_) => 3,
            },
        ));
    }
    run_method(engine, matches, name, args, names, input)
}

/// Opens the libraries this run names, or the app's choice when it names none,
/// and returns how people should hear each one called.
fn open_libraries(
    engine: &Engine,
    matches: &ArgMatches,
    name: &str,
    defaults: Defaults,
) -> Result<std::collections::BTreeMap<String, String>, Failure> {
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
    // `call` of search, show or cite reads what those commands would read.
    let called = matches
        .subcommand_matches("call")
        .and_then(|call| call.get_one::<String>("method"));
    let reads_app = matches!(name, "search" | "show" | "cite" | "entry" | "libraries")
        || called.is_some_and(|called| matches!(called.as_str(), "search" | "show" | "cite"));
    let app = if asked || (defaults == Defaults::App && !named && reads_app) {
        Some(bounded(app_libraries::load).unwrap_or_else(|| {
            Err(Failure::new(
                ErrorCode::SourceUnavailable,
                format!(
                    "the Ibis app's settings could not be read in time; try again, or {}.",
                    app_libraries::NAME_ONE
                ),
            ))
        })?)
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
        let params = serde_json::to_value(LibrariesRequest::OpenFile { path: path.clone() }).map_err(|_| {
            Failure::new(
                ErrorCode::BadInput,
                format!(
                    "cannot open {}: its name is not valid text; rename it.",
                    human::field(&path.display().to_string())
                ),
            )
        })?;
        let before = engine.libraries()?.libraries;
        let chosen = chosen_in(&path);
        // A path the app chose is never shown: agents read these messages too.
        explain(
            engine.dispatch(Method::Libraries, &params, &cancel),
            |EngineError { code, .. }| match chosen {
                Some(app) => format!(
                    "cannot open the library chosen in {app}: {} Choose it again there, or {}.",
                    human::file_reason(&path, code),
                    app_libraries::NAME_ONE
                ),
                None => human::file_failure(&path, code),
            },
        )
        .map_err(|mut failure| {
            if chosen.is_none() {
                failure.unnamed = Some(human::file_reason(&path, failure.error.code).to_owned());
            }
            failure
        })?;
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
        let profile = source_values::<String>(matches, "zotero-profile")
            .pop()
            .or_else(|| app.as_ref().and_then(|app| app.zotero_profile.clone()));
        let params = serde_json::to_value(LibrariesRequest::OpenZotero {
            libraries: Some(zotero),
            profile,
            reload: false,
        })
        .map_err(|_| EngineError::new(ErrorCode::BadInput))?;
        let before = engine.libraries()?.libraries;
        // A Zotero failure keeps its own wording with a reason too, so the
        // profiles a person can name are listed beside it.
        engine.dispatch(Method::Libraries, &params, &cancel).map_err(|error| {
            let profiles = if error.reason == Some(ErrorReason::ZoteroProfileNeeded) {
                zotero_profiles(engine, &cancel)
            } else {
                vec![]
            };
            zotero_open_failure(error, &profiles)
        })?;
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
    Ok(names)
}

/// Runs an engine method once its libraries are open.
fn run_method<R: BufRead>(
    engine: &Engine,
    matches: &ArgMatches,
    name: &str,
    args: &ArgMatches,
    names: std::collections::BTreeMap<String, String>,
    input: &mut R,
) -> Result<Execution, Failure> {
    let cancel = Cancellation::default();
    let method = Method::parse(name).map_err(EngineError::new)?;
    let libraries_open = || engine.libraries().is_ok_and(|result| !result.libraries.is_empty());
    let holdings = || match engine.libraries() {
        // Entries Ibis skipped are still references; the note about warnings says why they are absent.
        Ok(result)
            if result
                .libraries
                .iter()
                .any(|library| library.objects > 0 || library.warnings_total > 0) =>
        {
            human::Holdings::References
        }
        Ok(result) if !result.libraries.is_empty() => human::Holdings::Empty,
        _ => human::Holdings::NoLibrary,
    };
    let params = match method {
        Method::Libraries => json!({"action":"list"}),
        Method::Search => {
            let query = args.get_one::<String>("query").map_or("", String::as_str);
            let limit = args.get_one::<usize>("limit").copied().unwrap_or(10);
            if query.trim().is_empty() {
                return Err(Failure::new(
                    ErrorCode::Usage,
                    "give some words to search for: an author, title words, a year or a key.",
                ));
            }
            let result = explain(
                engine.dispatch(Method::Search, &json!({"query":query,"limit":limit}), &cancel),
                |EngineError { code, .. }| match code {
                    ErrorCode::BadInput => "the search words are too long; use fewer.".into(),
                    code => human::code_sentence(code).into(),
                },
            )?;
            return Ok(Execution::Result(
                human::with_summary(result, method, matches.get_flag("json"), holdings(), &names),
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
            let result = engine.dispatch(method, &params, &cancel).map_err(|error| match &key {
                Some(key) => key_failure(engine, key, error, libraries_open()),
                None if error.code == ErrorCode::NotFound => {
                    Failure::explained(error, "no open library holds the reference that --id names.")
                }
                None if error.code == ErrorCode::BadInput => Failure::explained(error, ITEM_REF_HELP),
                None => Failure::from(error),
            })?;
            return Ok(Execution::Result(
                human::with_summary(result, method, matches.get_flag("json"), holdings(), &names),
                None,
            ));
        }
        Method::Cite if args.get_flag("request") => {
            const REQUEST_HELP: &str = "stdin must hold one CitationRequest JSON object; see: ibis schema cite";
            let params = explain(stdin_json(input), |_| REQUEST_HELP.into())?;
            let citing = request_citing(&params).ok_or_else(|| Failure::new(ErrorCode::BadInput, REQUEST_HELP))?;
            let result = explain(
                engine.dispatch(method, &params, &cancel),
                |EngineError { code, .. }| match code {
                    ErrorCode::BadInput => REQUEST_HELP.into(),
                    code => human::code_sentence(code).into(),
                },
            )?;
            return Ok(Execution::Result(result, Some(citing)));
        }
        Method::Cite => {
            let project = writing_projects::chosen(args)?;
            let output = if let Some(project) = &project {
                ibis_engine::outputs::resolve(&project.output)?
            } else {
                match (args.get_one::<String>("style"), args.get_one::<String>("format")) {
                    (Some(style), _) => OutputSpec::Csl { style: style.clone() },
                    (None, format) => {
                        ibis_engine::outputs::find(format.map_or(ibis_engine::outputs::DEFAULT, String::as_str))
                            .map(|output| output.output.clone())
                            .ok_or_else(|| EngineError::new(ErrorCode::Internal))?
                    }
                }
            };
            if args.get_flag("bibliography") && !matches!(output, OutputSpec::Csl { .. }) {
                return Err(Failure::new(
                    ErrorCode::Usage,
                    "--bibliography needs a style: give --style, or a --format that is a style.",
                ));
            }
            let destination = args.get_one::<String>("for-app").map(|app| {
                let style = match &output {
                    OutputSpec::Csl { style } => style.as_str(),
                    _ => ibis_engine::outputs::DEFAULT_STYLE,
                };
                destination(app, System::HERE, &output, style)
            });
            let output = destination
                .as_ref()
                .map_or(output, |destination| destination.output.clone());
            let form = if args.get_flag("bibliography") {
                CitationForm::BibliographyOnly
            } else {
                serde_json::from_value(json!(
                    args.get_one::<String>("form").map(String::as_str).unwrap_or("normal")
                ))
                .map_err(|_| EngineError::new(ErrorCode::Usage))?
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
                output: output_name(&output),
                form,
            };
            let writing_project = project
                .as_ref()
                .map(|project| {
                    engine.cite_writing_project_keys_with_policy(
                        project,
                        &KeyCitationRequest {
                            keys: keys.clone(),
                            output: output.clone(),
                            form,
                            output_locale: args.get_one::<String>("locale").cloned(),
                        },
                        writing_projects::policy(args),
                        &cancel,
                    )
                })
                .transpose()?;
            let mut result = if let Some(project) = &writing_project {
                Ok(project.citation.clone())
            } else {
                engine.cite_keys(
                    &KeyCitationRequest {
                        keys: keys.clone(),
                        output,
                        form,
                        output_locale: args.get_one::<String>("locale").cloned(),
                    },
                    &cancel,
                )
            }
            .map_err(|error| match error.reason {
                Some(ErrorReason::UnknownStyle) => Failure::explained(
                    error,
                    format!(
                        "there is no style '{}'; list the styles with: ibis styles",
                        human::field(args.get_one::<String>("style").map_or("", String::as_str))
                    ),
                ),
                Some(ErrorReason::UnknownLocale) => Failure::explained(
                    error,
                    format!(
                        "there is no locale '{}'; list them with: ibis styles --locales",
                        human::field(args.get_one::<String>("locale").map_or("", String::as_str))
                    ),
                ),
                // The error names no key; the first key that fails the same way is the one.
                _ => keys
                    .iter()
                    .map(|item| &item.key)
                    .find(|key| {
                        engine
                            .dispatch(Method::Show, &json!({"item":{"by":"key","value":key}}), &cancel)
                            .err()
                            == Some(error)
                    })
                    .map_or_else(
                        || Failure::from(error),
                        |key| key_failure(engine, key, error, libraries_open()),
                    ),
            })?;
            result.destination = destination;
            let result = engine.finalize(DispatchResult::citation(result, keys.len())?)?;
            let result = writing_projects::with_check(result, writing_project)?;
            return Ok(Execution::Result(result, Some(citing)));
        }
        Method::Project => {
            let path = args.get_one::<PathBuf>("bib").cloned().unwrap_or_default();
            let keys: Vec<&String> = args.get_many::<String>("keys").into_iter().flatten().collect();
            let result = explain(
                engine.dispatch(method, &json!({"path":path,"keys":keys}), &cancel),
                |EngineError { code, .. }| match code {
                    ErrorCode::BadInput => "at most 1000 keys can be checked at once.".into(),
                    code => human::code_sentence(code).into(),
                },
            )?;
            return Ok(Execution::Result(
                human::with_summary(result, method, matches.get_flag("json"), holdings(), &names),
                None,
            ));
        }
        Method::Entry => {
            let key = args.get_one::<String>("key").cloned().unwrap_or_default();
            let result = engine
                .dispatch(method, &json!({"item":{"by":"key","value":key}}), &cancel)
                .map_err(|error| key_failure(engine, &key, error, libraries_open()))?;
            return Ok(Execution::Result(
                human::with_summary(result, method, matches.get_flag("json"), holdings(), &names),
                None,
            ));
        }
        Method::Styles => {
            let words: Vec<&str> = args
                .get_many::<String>("query")
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect();
            json!({
                "query": (!words.is_empty()).then(|| words.join(" ")),
                "limit": args.get_one::<usize>("limit").copied().unwrap_or(100),
                "offset": args.get_one::<usize>("offset").copied().unwrap_or(0),
            })
        }
        Method::Outputs => json!({}),
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
                |EngineError { code, .. }| match code {
                    ErrorCode::BadInput => params_help(),
                    code => human::code_sentence(code).into(),
                },
            )?;
            let citing = (called == Method::Cite).then(|| request_citing(&params)).flatten();
            return Ok(Execution::Result(result, citing));
        }
    };
    let result = explain(
        engine.dispatch(method, &params, &cancel),
        |EngineError { code, .. }| match code {
            ErrorCode::BadInput
                if method == Method::Styles && params["query"].as_str().is_some_and(|query| query.len() > 1024) =>
            {
                "the search words are too long; use fewer.".into()
            }
            ErrorCode::BadInput if method == Method::Styles => "--limit must be between 1 and 20000.".into(),
            code => human::code_sentence(code).into(),
        },
    )?;
    if method == Method::Styles && args.get_flag("locales") && !matches.get_flag("json") {
        return Ok(Execution::Result(human::locale_list(result), None));
    }
    Ok(Execution::Result(
        human::with_summary(result, method, matches.get_flag("json"), holdings(), &names),
        None,
    ))
}
/// What a canonical CitationRequest asked for, so its findings read as sentences.
fn request_citing(params: &Value) -> Option<human::Citing> {
    let request: CitationRequest = serde_json::from_value(params.clone()).ok()?;
    Some(human::Citing {
        labels: vec![],
        flags: vec![],
        output: output_name(&request.output),
        form: request.form,
    })
}
/// An output's one name from the engine's list; a style outside it is named by its id.
fn output_name(output: &OutputSpec) -> String {
    match (ibis_engine::outputs::named(output), output) {
        (Some(entry), _) => entry.name.clone(),
        (None, OutputSpec::Csl { style }) => style.clone(),
        (None, OutputSpec::Preset { name }) => name.clone(),
        (None, OutputSpec::Key { syntax, .. }) => serde_json::to_value(syntax)
            .ok()
            .and_then(|syntax| syntax.as_str().map(str::to_owned))
            .unwrap_or_default(),
    }
}
/// `--format` takes exactly the engine's output ids, each shown by its name.
fn output_ids() -> clap::builder::PossibleValuesParser {
    clap::builder::PossibleValuesParser::new(
        ibis_engine::outputs::outputs()
            .iter()
            .map(|output| clap::builder::PossibleValue::new(output.id.as_str()).help(output.name.as_str())),
    )
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
    ibis_core::locator::read_given(text).ok_or_else(|| {
        Failure::new(
            ErrorCode::Usage,
            format!(
                "--locator '{}' is not a locator Ibis reads; write it like \"p. 12\", \"ch. 3\" or \"12-14\".",
                human::field(text)
            ),
        )
    })
}
const ITEM_REF_HELP: &str = "--id must be a reference's id, the JSON that ibis search --json prints.";
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
    protocol::parse_json(bytes).map_err(|_| EngineError::new(ErrorCode::BadInput))
}
fn stdin_json(input: &mut impl BufRead) -> Result<Value, EngineError> {
    let mut bytes = vec![];
    input
        .take(protocol::MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| EngineError::new(ErrorCode::BadInput))?;
    parse_json(&bytes)
}
fn emit(
    json: bool,
    result: Result<DispatchResult, Failure>,
    citing: Option<&human::Citing>,
    libraries: &str,
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
        let json_flag = if json { " --json" } else { "" };
        say(&format!(
            "note: a library has warnings or could not refresh; see: ibis libraries{libraries}{json_flag}"
        ));
    }
    if !json && let (Ok(result), Some(citing)) = (&result, citing) {
        let citation = result.data.get("citation").unwrap_or(&result.data);
        let project_blocked = result
            .data
            .pointer("/assessment/state")
            .and_then(Value::as_str)
            .is_some_and(|state| matches!(state, "paused" | "conflict"));
        if !project_blocked {
            for line in human::citation_report(citation, citing, result.text.is_some()) {
                say(&line);
            }
        }
        if let Some(project) = result.data.get("project").filter(|value| !value.is_null())
            && let Some(report) = human::project_report(project)
        {
            say(&report);
        }
        if let Some(message) = result.data.pointer("/assessment/message").and_then(Value::as_str) {
            say(message);
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
                .unwrap_or_else(|| human::code_sentence(failure.error.code)));
            if !json {
                return failure.error.code.exit_code().value();
            }
            (failure.error.code.exit_code(), Envelope::failure(failure.error), None)
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
    if let Err(error) = output
        .write_all(&encoded)
        .and_then(|()| output.write_all(b"\n"))
        .and_then(|()| output.flush())
    {
        // A reader that closed the pipe early (`| head`) asked for no more, so nothing went wrong.
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            return code.value();
        }
        say(human::write_failure(&error));
        return 70;
    }
    code.value()
}

/// Terminal presentation escapes control sequences and invisible direction overrides;
/// canonical strings in --json output remain byte-for-byte unchanged.
fn terminal_display(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        if human::is_hidden(character) || matches!(character, '\u{200e}' | '\u{200f}') {
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
    fn a_stalled_wait_gives_up_at_its_deadline() {
        assert_eq!(bounded(|| 7), Some(7));
        let started = std::time::Instant::now();
        let stalled = bounded(|| std::thread::sleep(std::time::Duration::from_secs(30)));
        assert_eq!(stalled, None);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn an_agent_never_reads_the_zotero_profile_names_a_person_does() {
        let error = EngineError::because(ErrorCode::SourceUnavailable, ErrorReason::ZoteroProfileNeeded);
        let failure = zotero_open_failure(error, &["Invented Home".into(), "Invented Work".into()]);
        assert!(failure.sentence().contains("Profiles: Invented Home, Invented Work."));
        let refusal = format!("{:?}", failure.refusal());
        assert!(!refusal.contains("Invented"), "{refusal}");
        assert!(!refusal.contains("Profiles"), "{refusal}");
        assert!(refusal.contains("--zotero-profile"), "{refusal}");
        // Without names to hide, both readers get the same sentence.
        let plain = zotero_open_failure(EngineError::new(ErrorCode::SourceUnavailable), &[]);
        assert!(format!("{:?}", plain.refusal()).contains(plain.sentence()));
    }

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
    fn output_that_cannot_be_written_is_explained_unless_the_reader_left() {
        struct Failing(std::io::ErrorKind);
        impl std::io::Write for Failing {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(self.0.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let result = || DispatchResult {
            data: json!({"invented":true}),
            text: Some("(Lee, 2024)".into()),
            source_findings: false,
            exit_code: ibis_core::contract::ExitCode::Ok,
        };
        for (kind, expected) in [
            (
                std::io::ErrorKind::StorageFull,
                "ibis: the output could not be written because the disk is full; free some space and try again.\n",
            ),
            (
                std::io::ErrorKind::PermissionDenied,
                "ibis: the output could not be written; check where it is sent and try again.\n",
            ),
            (std::io::ErrorKind::BrokenPipe, ""),
        ] {
            for json in [false, true] {
                let mut errors = Vec::new();
                let code = emit(
                    json,
                    Ok(result()),
                    None,
                    "",
                    &mut Failing(kind),
                    &mut errors,
                    OutputMode::Plain,
                );
                assert_eq!(code, if expected.is_empty() { 0 } else { 70 });
                assert_eq!(String::from_utf8_lossy(&errors), expected, "{kind:?} json={json}");
            }
        }
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
            emit(
                true,
                Ok(result),
                None,
                "",
                &mut Broken,
                &mut Vec::new(),
                OutputMode::Plain
            ),
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
