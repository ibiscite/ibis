//! Writings use the same checked settings model as the app. CLI
//! selection belongs to one request, never the GUI's selected app or document.
use crate::{Failure, human, settings_backup};
use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::{ErrorCode, ExitCode, OutputSpec};
use ibis_engine::settings::{
    self,
    writings::{self, Writing, WritingError},
};
use ibis_engine::{DispatchResult, Engine, ProjectAssessmentState, WritingCitation, request::StylesRequest};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(crate) fn command() -> Command {
    let id = || {
        Arg::new("id")
            .value_name("NAME")
            .required(true)
            .help("The writing name printed by ibis writings list")
    };
    let name = || {
        Arg::new("name")
            .value_name("NAME")
            .required(true)
            .help("A unique name for this writing")
    };
    let format = || {
        Arg::new("format")
            .long("format")
            .value_name("FORMAT")
            .help("Output ID from ibis outputs: a key syntax or a common style")
            .hide_possible_values(true)
            .value_parser(crate::output_ids())
            .conflicts_with("style")
    };
    let style = || {
        Arg::new("style")
            .long("style")
            .value_name("STYLE_ID")
            .help("Any bundled CSL style ID; list them with: ibis styles")
    };
    let bibliography = || {
        Arg::new("bibliography")
            .long("bibliography")
            .value_name("FILE")
            .help("The read-only writing bibliography to check, as a .bib file")
            .value_parser(clap::value_parser!(PathBuf))
    };
    Command::new("writings")
        .about("Remember a writing's citation output and optional bibliography")
        .after_help("Writings stay on this computer. Their bibliography is read-only; no writing choice grants paste permission.\nUse a writing for one citation: ibis cite KEY --writing NAME")
        .subcommand_required(true).arg_required_else_help(true)
        .arg(Arg::new("nightly").long("nightly").global(true).action(ArgAction::SetTrue).help("Use Ibis Nightly's writing definitions"))
        .subcommand(Command::new("list").about("List writing names, formats and bibliography file names"))
        .subcommand(Command::new("schema").about("The JSON Schema of cite --writing's result"))
        .subcommand(Command::new("create").about("Create a writing").arg(name()).arg(format().required_unless_present("style")).arg(style()).arg(bibliography()))
        .subcommand(Command::new("rename").about("Rename a writing").arg(id()).arg(name()))
        .subcommand(Command::new("remove").about("Remove a writing and its app selections; keep its bibliography file").arg(id()))
        .subcommand(Command::new("update").about("Change a writing's output or bibliography").arg(id()).arg(format()).arg(style()).arg(bibliography().conflicts_with("no-bibliography"))
            .arg(Arg::new("no-bibliography").long("no-bibliography").action(ArgAction::SetTrue).help("Stop checking a bibliography in this writing")))
}

fn failure(error: WritingError) -> Failure {
    let code = match &error {
        WritingError::NotFound => ErrorCode::NotFound,
        WritingError::ContextUnavailable => ErrorCode::SourceUnavailable,
        WritingError::Settings(error) => ibis_engine::settings_backup::settings_code(error),
        _ => ErrorCode::BadInput,
    };
    let message = match &error {
        WritingError::Settings(problem) => ibis_engine::settings_backup::settings_message(problem),
        error => error.to_string(),
    };
    Failure::new(code, message)
}

fn load(nightly: bool) -> Result<(PathBuf, settings::Settings), Failure> {
    let (_, path) = settings_backup::settings_file(nightly)?;
    let loaded = settings::load(&path);
    let settings = loaded.cites().map_err(|problem| {
        Failure::new(
            ibis_engine::settings_backup::settings_code(problem),
            ibis_engine::settings_backup::settings_message(problem),
        )
    })?;
    Ok((path, settings.clone()))
}

pub(crate) struct Context {
    pub writing: Option<Writing>,
    pub settings: Option<settings::Settings>,
}

pub(crate) fn chosen(args: &ArgMatches) -> Result<Context, Failure> {
    let name = args.get_one::<String>("writing");
    let document = args.get_one::<PathBuf>("document");
    let app = args.get_one::<String>("for-app");
    if name.is_none() && document.is_none() && app.is_none() {
        return Ok(Context {
            writing: None,
            settings: None,
        });
    }
    let settings = if app.is_some() {
        crate::app_libraries::format_settings(args.get_flag("nightly"))?
    } else {
        load(args.get_flag("nightly"))?.1
    };
    let writing = if let Some(name) = name {
        Some(writings::find(&settings, name).cloned().map_err(failure)?)
    } else if let Some(app) = app {
        let app = ibis_core::destination::AppId::named(app, ibis_core::destination::System::HERE);
        for_app(&settings, &app)?
    } else {
        writings::from_context(&settings, document.map(PathBuf::as_path), None, None).map_err(failure)?
    };
    Ok(Context {
        writing,
        settings: Some(settings),
    })
}

pub(crate) fn for_app(
    settings: &settings::Settings,
    app: &ibis_core::destination::AppId,
) -> Result<Option<Writing>, Failure> {
    let chosen = ibis_engine::format::choose(settings, Some(app));
    chosen
        .writing
        .as_ref()
        .map(|writing| writings::find(settings, &writing.name).cloned().map_err(failure))
        .transpose()
}

fn output(engine: &Engine, args: &ArgMatches) -> Result<Option<OutputSpec>, Failure> {
    if let Some(style) = args.get_one::<String>("style") {
        let styles = engine.styles(&StylesRequest {
            limit: 20_000,
            ..StylesRequest::default()
        })?;
        if !styles.styles.iter().any(|known| &known.id == style) {
            return Err(Failure::new(
                ErrorCode::BadInput,
                "That citation style is unavailable. List styles with: ibis styles",
            ));
        }
        return Ok(Some(OutputSpec::Csl { style: style.clone() }));
    }
    args.get_one::<String>("format")
        .map(|format| {
            ibis_engine::outputs::find(format)
                .map(|output| output.output.clone())
                .ok_or_else(|| {
                    Failure::new(
                        ErrorCode::BadInput,
                        "That output is unavailable. List outputs with: ibis outputs",
                    )
                })
        })
        .transpose()
}

fn bibliography(args: &ArgMatches) -> Result<Option<PathBuf>, Failure> {
    args.get_one::<PathBuf>("bibliography")
        .map(|path| {
            if path.file_name().is_none()
                || !path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("bib"))
            {
                return Err(Failure::new(
                    ErrorCode::BadInput,
                    "Choose a BibTeX or BibLaTeX bibliography with a .bib file name.",
                ));
            }
            if path.is_absolute() {
                Ok(path.clone())
            } else {
                std::env::current_dir().map(|current| current.join(path)).map_err(|_| {
                    Failure::new(
                        ErrorCode::SourceUnavailable,
                        "The current folder is unavailable. Give the bibliography's full path.",
                    )
                })
            }
        })
        .transpose()
}

fn view(writing: &Writing) -> Value {
    json!({"name":writing.name,"format":writing.format,
        "bibliography":writing.bibliography.as_ref().and_then(|path| path.file_name()).map(|name| name.to_string_lossy())})
}

pub(crate) fn run(engine: &Engine, args: &ArgMatches) -> Result<DispatchResult, Failure> {
    let nightly = args.get_flag("nightly");
    let (action, args) = args
        .subcommand()
        .ok_or_else(|| Failure::new(ErrorCode::Usage, "Choose list, create, rename, update or remove."))?;
    if action == "schema" {
        return Ok(DispatchResult {
            data: WritingCitation::schema(),
            text: None,
            source_findings: false,
            exit_code: ExitCode::Ok,
        });
    }
    let (path, settings) = load(nightly)?;
    let result = if action == "list" {
        let rows: Vec<_> = settings.device.writings.iter().map(view).collect();
        let text = if rows.is_empty() {
            "No writings yet. Create one with: ibis writings create NAME --format pandoc".into()
        } else {
            settings
                .device
                .writings
                .iter()
                .map(|writing| {
                    let bibliography = writing
                        .bibliography
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map(|name| human::field(&name.to_string_lossy()))
                        .unwrap_or_else(|| {
                            ibis_engine::catalogue::text("cli-writing-without-bibliography", None, &[])
                                .unwrap_or_default()
                        });
                    format!(
                        "{}\t{}\t{bibliography}",
                        human::field(&writing.name),
                        crate::output_name(&writing.format)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        return Ok(DispatchResult {
            data: json!({"writings": rows}),
            text: Some(text),
            source_findings: false,
            exit_code: ExitCode::Ok,
        });
    } else {
        let owner = settings::Owner::open(path);
        let id = args
            .try_get_one::<String>("id")
            .ok()
            .flatten()
            .map(String::as_str)
            .unwrap_or("");
        match action {
            "create" => {
                let output = output(engine, args)?
                    .ok_or_else(|| Failure::new(ErrorCode::Usage, "Choose --format or --style."))?;
                let bibliography = bibliography(args)?;
                let name = args.get_one::<String>("name").cloned().unwrap_or_default();
                owner.change(|settings| writings::create(settings, name, output, bibliography))
            }
            "rename" => {
                let name = args.get_one::<String>("name").cloned().unwrap_or_default();
                owner.change(|settings| writings::rename(settings, id, name))
            }
            "remove" => owner.change(|settings| writings::remove(settings, id)),
            "update" => {
                let output = output(engine, args)?;
                let bibliography = bibliography(args)?;
                let clear = args.get_flag("no-bibliography");
                owner.change(|settings| {
                    let old = writings::find(settings, id)?.clone();
                    writings::update(
                        settings,
                        id,
                        output.unwrap_or(old.format),
                        if clear { None } else { bibliography.or(old.bibliography) },
                    )
                })
            }
            _ => {
                return Err(Failure::new(
                    ErrorCode::Usage,
                    "Choose list, create, rename, update or remove.",
                ));
            }
        }
        .map(|(_, writing)| writing)
        .map_err(failure)?
    };
    Ok(DispatchResult {
        data: json!({"writing":view(&result),"action":action}),
        text: Some(format!(
            "{} writing {}.{}",
            match action {
                "create" => "Created",
                "rename" => "Renamed",
                "remove" => "Removed",
                _ => "Updated",
            },
            human::field(&result.name),
            if action == "remove" {
                " Its bibliography file was kept."
            } else {
                ""
            }
        )),
        source_findings: false,
        exit_code: ExitCode::Ok,
    })
}

pub(crate) fn with_check(
    mut result: DispatchResult,
    writing: Option<WritingCitation>,
) -> Result<DispatchResult, Failure> {
    let Some(writing) = writing else {
        return Ok(result);
    };
    if writing.bibliography.as_ref().is_some_and(|check| !check.settled())
        || writing
            .assessment
            .as_ref()
            .is_some_and(|assessment| assessment.state != ProjectAssessmentState::Ready)
    {
        result.exit_code = ExitCode::Findings;
    }
    result.data = serde_json::to_value(writing)
        .map_err(|_| Failure::new(ErrorCode::Internal, "The writing citation is unavailable."))?;
    Ok(result.bounded()?)
}
