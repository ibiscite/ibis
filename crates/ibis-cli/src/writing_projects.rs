//! Writing projects use the same checked settings model as the app. CLI
//! selection belongs to one request, never the GUI's selected app or document.
use crate::{Failure, human, settings_backup};
use clap::{Arg, ArgAction, ArgMatches, Command};
use ibis_core::contract::{ErrorCode, ExitCode, OutputSpec};
use ibis_core::settings::{
    self,
    writing_projects::{self as projects, ProjectError, WritingProject},
};
use ibis_engine::{
    DispatchResult, Engine, ProjectAssessmentState, ProjectPolicy, WritingProjectCitation, request::StylesRequest,
};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(crate) fn command() -> Command {
    let id = || {
        Arg::new("id")
            .value_name("PROJECT_ID")
            .required(true)
            .help("The stable project ID printed by ibis projects list")
    };
    let name = || {
        Arg::new("name")
            .value_name("NAME")
            .required(true)
            .help("A unique name for this writing project")
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
            .help("The read-only project bibliography to check, as a .bib file")
            .value_parser(clap::value_parser!(PathBuf))
    };
    Command::new("projects")
        .about("Remember a writing project's citation output and optional bibliography")
        .after_help("Projects stay on this computer. Their bibliography is read-only; no project choice grants paste permission.\nUse a project for one citation: ibis cite KEY --project PROJECT_ID")
        .subcommand_required(true).arg_required_else_help(true)
        .arg(Arg::new("nightly").long("nightly").global(true).action(ArgAction::SetTrue).help("Use Ibis Nightly's project definitions"))
        .subcommand(Command::new("list").about("List project IDs, names, outputs and bibliography file names"))
        .subcommand(Command::new("schema").about("The JSON Schema of cite --project's result"))
        .subcommand(Command::new("create").about("Create a writing project").arg(name()).arg(format().required_unless_present("style")).arg(style()).arg(bibliography()))
        .subcommand(Command::new("rename").about("Rename a project, keeping its ID").arg(id()).arg(name()))
        .subcommand(Command::new("remove").about("Remove a project and its app selections; keep its bibliography file").arg(id()))
        .subcommand(Command::new("update").about("Change a project's output or bibliography").arg(id()).arg(format()).arg(style()).arg(bibliography().conflicts_with("no-bibliography"))
            .arg(Arg::new("no-bibliography").long("no-bibliography").action(ArgAction::SetTrue).help("Stop checking a bibliography in this project")))
}

fn failure(error: ProjectError) -> Failure {
    let code = match &error {
        ProjectError::NotFound => ErrorCode::NotFound,
        ProjectError::Settings(settings::SettingsError::NewerVersion(_)) => ErrorCode::NewerFormat,
        ProjectError::Settings(_) => ErrorCode::SourceUnavailable,
        _ => ErrorCode::BadInput,
    };
    Failure::new(code, error.to_string())
}

fn load(nightly: bool) -> Result<(PathBuf, settings::Settings), Failure> {
    let (_, path) = settings_backup::settings_file(nightly)?;
    let loaded = settings::load(&path);
    if let Some(problem) = loaded.problem {
        return Err(failure(ProjectError::Settings(problem)));
    }
    Ok((path, loaded.settings))
}

pub(crate) fn chosen(args: &ArgMatches) -> Result<Option<WritingProject>, Failure> {
    let Some(id) = args.get_one::<String>("project") else {
        return Ok(None);
    };
    let (_, settings) = load(args.get_flag("nightly"))?;
    projects::find(&settings, id).cloned().map(Some).map_err(failure)
}

pub(crate) fn policy(args: &ArgMatches) -> ProjectPolicy {
    if args.get_flag("strict-project") {
        ProjectPolicy::Strict
    } else {
        ProjectPolicy::Normal
    }
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

fn view(project: &WritingProject) -> Value {
    json!({"id":project.id,"name":project.name,"output":project.output,
        "bibliography":project.bibliography.as_ref().and_then(|path| path.file_name()).map(|name| name.to_string_lossy()),
        "revision":project.revision()})
}

pub(crate) fn run(engine: &Engine, args: &ArgMatches) -> Result<DispatchResult, Failure> {
    let nightly = args.get_flag("nightly");
    let (action, args) = args
        .subcommand()
        .ok_or_else(|| Failure::new(ErrorCode::Usage, "Choose list, create, rename, update or remove."))?;
    if action == "schema" {
        return Ok(DispatchResult {
            data: WritingProjectCitation::schema(),
            text: None,
            source_findings: false,
            exit_code: ExitCode::Ok,
        });
    }
    let (path, settings) = load(nightly)?;
    let result = if action == "list" {
        let rows: Vec<_> = settings.writing_projects.iter().map(view).collect();
        let text = if rows.is_empty() {
            "No writing projects yet. Create one with: ibis projects create NAME --format pandoc".into()
        } else {
            settings
                .writing_projects
                .iter()
                .map(|project| {
                    let bibliography = project
                        .bibliography
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map(|name| human::field(&name.to_string_lossy()))
                        .unwrap_or_else(|| {
                            ibis_engine::catalogue::text("cli-project-without-bibliography", None, &[])
                                .unwrap_or_default()
                        });
                    format!(
                        "{}\t{}\t{}\t{bibliography}",
                        project.id,
                        human::field(&project.name),
                        crate::output_name(&project.output)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        return Ok(DispatchResult {
            data: json!({"projects": rows}),
            text: Some(text),
            source_findings: false,
            exit_code: ExitCode::Ok,
        });
    } else {
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
                projects::change(&path, |settings| projects::create(settings, name, output, bibliography))
            }
            "rename" => {
                let name = args.get_one::<String>("name").cloned().unwrap_or_default();
                projects::change(&path, |settings| projects::rename(settings, id, name))
            }
            "remove" => projects::change(&path, |settings| projects::remove(settings, id)),
            "update" => {
                let output = output(engine, args)?;
                let bibliography = bibliography(args)?;
                let clear = args.get_flag("no-bibliography");
                projects::change(&path, |settings| {
                    let old = projects::find(settings, id)?.clone();
                    projects::update(
                        settings,
                        id,
                        output.unwrap_or(old.output),
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
        .map_err(failure)?
    };
    Ok(DispatchResult {
        data: json!({"project":view(&result),"action":action}),
        text: Some(format!(
            "{} writing project {}.{}",
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
    project: Option<WritingProjectCitation>,
) -> Result<DispatchResult, Failure> {
    let Some(project) = project else {
        return Ok(result);
    };
    if project.project.as_ref().is_some_and(|check| !check.settled())
        || project
            .assessment
            .as_ref()
            .is_some_and(|assessment| assessment.state != ProjectAssessmentState::Ready)
    {
        result.exit_code = ExitCode::Findings;
    }
    result.data = serde_json::to_value(project)
        .map_err(|_| Failure::new(ErrorCode::Internal, "The project citation is unavailable."))?;
    Ok(result)
}
