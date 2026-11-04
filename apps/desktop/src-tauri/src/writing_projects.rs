//! Explicit writing contexts. A saved definition and the app's selection are
//! committed together; selecting one never grants delivery permission.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ibis_core::destination::{AppId, System};
use ibis_core::settings::writing_projects::{self as projects, ProjectError, WritingProject};
use ibis_core::settings::{self, Settings};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::destination::AppOutput;
use crate::preferences::SettingsFile;

const CHANGED: &str = "The writing project changed. Open the project menu again.";
const REOPEN: &str = "The destination changed. Open the project menu again.";
const EDITOR: &str = "This editor request chooses its own output. Finish it before changing writing projects.";

/// The only selection that is not saved: no app was captured for it.
#[derive(Default)]
pub(crate) struct Runtime(Mutex<Option<String>>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct WritingProjectView {
    pub(crate) id: String,
    pub(crate) name: String,
    /// A CLI project can name outputs not yet available in the picker.
    pub(crate) output: Option<AppOutput>,
    pub(crate) bibliography: Option<String>,
    pub(crate) revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct WritingProjectsView {
    pub(crate) projects: Vec<WritingProjectView>,
    pub(crate) selected: Option<String>,
    pub(crate) session_only: bool,
    /// Owns one picker opening and captured destination; never a path or title.
    pub(crate) context: String,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum WritingProjectEdit {
    Create {
        name: String,
        output: AppOutput,
    },
    Rename {
        id: String,
        revision: String,
        name: String,
    },
    Remove {
        id: String,
        revision: String,
    },
    Select {
        id: Option<String>,
    },
    Output {
        id: String,
        revision: String,
        output: AppOutput,
    },
}

#[derive(Debug)]
enum ChangeError {
    Project(ProjectError),
    Stale,
    Destination,
    Editor,
    Unsupported,
}

impl fmt::Display for ChangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => error.fmt(formatter),
            Self::Stale => formatter.write_str(CHANGED),
            Self::Destination => formatter.write_str(REOPEN),
            Self::Editor => formatter.write_str(EDITOR),
            Self::Unsupported => {
                formatter.write_str("This project's output is not available in the picker. Choose another format.")
            }
        }
    }
}
impl From<ProjectError> for ChangeError {
    fn from(error: ProjectError) -> Self {
        Self::Project(error)
    }
}
impl From<settings::SettingsError> for ChangeError {
    fn from(error: settings::SettingsError) -> Self {
        Self::Project(ProjectError::Settings(error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    binding: Option<String>,
    generation: u64,
    editor: bool,
}
impl Target {
    fn context(&self) -> String {
        // Stable app IDs and a local generation only; no document information.
        serde_json::to_string(&(self.generation, &self.binding, self.editor)).unwrap_or_default()
    }
}

pub(crate) fn app_key(id: &AppId) -> String {
    let system = match id.system {
        System::Macos => "macos",
        System::Windows => "windows",
        System::Linux => "linux",
    };
    format!("{system}:{}", id.normalized())
}

fn target(app: &AppHandle, captured: Option<&AppId>) -> Target {
    let editor = app
        .try_state::<crate::editor_endpoint::Pending>()
        .is_some_and(|pending| pending.current().is_some());
    Target {
        binding: if editor { None } else { captured.map(app_key) },
        generation: app
            .try_state::<crate::picker::PickerGeneration>()
            .map_or(0, |generation| generation.current()),
        editor,
    }
}

fn current_target(app: &AppHandle) -> Target {
    target(app, crate::paste::last_app(app).as_ref().map(|(id, _, _)| id))
}

fn context_token(app: &AppHandle, target: &Target) -> String {
    let activation = app.state::<crate::project::ProjectBibliography>().version().0;
    context_token_for(target, activation)
}

fn context_token_for(target: &Target, activation: u32) -> String {
    serde_json::to_string(&(target.context(), activation)).unwrap_or_default()
}

fn read_settings(path: &Path) -> Result<Settings, String> {
    let loaded = settings::load(path);
    // The existing settings recovery policy treats a validated backup as the
    // chosen settings, with its warning shown in Settings. With no valid copy,
    // never turn an unreadable selected project into a default-format paste.
    if loaded.problem.is_some() && !loaded.from_backup {
        return Err("Writing projects could not be read. Check Settings and try again.".to_owned());
    }
    Ok(loaded.settings)
}

fn load_settings(app: &AppHandle, target: &Target) -> Result<Settings, String> {
    let loaded = read_settings(&app.state::<SettingsFile>().path);
    if loaded.is_err() {
        crate::project::activate_context(
            app,
            format!(
                "unavailable:settings:{}",
                target.binding.as_deref().unwrap_or("session")
            ),
            None,
            None,
        );
    }
    loaded
}

fn selected<'a>(settings: &'a Settings, target: &Target, session: Option<&'a str>) -> Option<&'a str> {
    if target.editor {
        return None;
    }
    target.binding.as_ref().map_or(session, |key| {
        settings.writing_project_apps.get(key).map(String::as_str)
    })
}

fn view_for(settings: &Settings, target: &Target, session: Option<&str>) -> WritingProjectsView {
    WritingProjectsView {
        projects: settings
            .writing_projects
            .iter()
            .map(|project| WritingProjectView {
                id: project.id.clone(),
                name: project.name.clone(),
                output: AppOutput::from_project_spec(&project.output).ok(),
                bibliography: project.bibliography.as_deref().map(ibis_engine::project_name),
                revision: project.revision(),
            })
            .collect(),
        selected: selected(settings, target, session).map(str::to_owned),
        session_only: target.binding.is_none(),
        context: target.context(),
    }
}

/// Native-only context. The bibliography path never enters an IPC view.
#[derive(Debug, Clone)]
pub(crate) struct ActiveProject {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) output: AppOutput,
    pub(crate) bibliography: Option<PathBuf>,
    pub(crate) revision: String,
    pub(crate) context_key: String,
}

impl ActiveProject {
    pub(crate) fn view(&self) -> WritingProjectView {
        WritingProjectView {
            id: self.id.clone(),
            name: self.name.clone(),
            output: Some(self.output),
            bibliography: self.bibliography.as_deref().map(ibis_engine::project_name),
            revision: self.revision.clone(),
        }
    }
}

fn active_for(settings: &Settings, target: &Target, session: Option<&str>) -> Result<Option<ActiveProject>, String> {
    let Some(id) = selected(settings, target, session) else {
        return Ok(None);
    };
    let project = projects::find(settings, id).map_err(|error| error.to_string())?;
    let revision = project.revision();
    Ok(Some(ActiveProject {
        id: project.id.clone(),
        name: project.name.clone(),
        output: AppOutput::from_project_spec(&project.output)?,
        bibliography: project.bibliography.clone(),
        revision: revision.clone(),
        context_key: format!(
            "project:{}:{}:{revision}",
            target.binding.as_deref().unwrap_or("session"),
            project.id
        ),
    }))
}

fn checked<'a>(settings: &'a Settings, id: &str, revision: &str) -> Result<&'a WritingProject, ChangeError> {
    let project = projects::find(settings, id)?;
    if project.revision() != revision {
        return Err(ChangeError::Stale);
    }
    Ok(project)
}

/// The result is the replacement runtime-only choice. Definitions and saved
/// app bindings are changed on the transaction's private copy, never early.
fn edit(
    settings: &mut Settings,
    target: &Target,
    session: Option<&str>,
    action: &WritingProjectEdit,
) -> Result<Option<String>, ChangeError> {
    if target.editor {
        return Err(ChangeError::Editor);
    }
    let mut session = session.map(str::to_owned);
    match action {
        WritingProjectEdit::Create { name, output } => {
            let project = projects::create(settings, name.clone(), output.project_spec(), None)?;
            select(settings, target, &mut session, Some(project.id));
        }
        WritingProjectEdit::Select { id } => {
            if let Some(id) = id {
                let project = projects::find(settings, id)?;
                AppOutput::from_project_spec(&project.output).map_err(|_| ChangeError::Unsupported)?;
            }
            select(settings, target, &mut session, id.clone());
        }
        WritingProjectEdit::Rename { id, revision, name } => {
            checked(settings, id, revision)?;
            projects::rename(settings, id, name.clone())?;
        }
        WritingProjectEdit::Remove { id, revision } => {
            checked(settings, id, revision)?;
            projects::remove(settings, id)?;
            if session.as_deref() == Some(id) {
                session = None;
            }
        }
        WritingProjectEdit::Output { id, revision, output } => {
            let bibliography = checked(settings, id, revision)?.bibliography.clone();
            projects::update(settings, id, output.project_spec(), bibliography)?;
        }
    }
    Ok(session)
}

fn select(settings: &mut Settings, target: &Target, session: &mut Option<String>, id: Option<String>) {
    if let Some(key) = &target.binding {
        if let Some(id) = id {
            settings.writing_project_apps.insert(key.clone(), id);
        } else {
            settings.writing_project_apps.remove(key);
        }
    } else {
        *session = id;
    }
}

fn activate(
    app: &AppHandle,
    settings: &Settings,
    target: &Target,
    session: Option<&str>,
) -> Result<Option<ActiveProject>, String> {
    // Editor requests own their output and do not change the GUI's context.
    if target.editor {
        return Ok(None);
    }
    let active = match active_for(settings, target, session) {
        Ok(active) => active,
        Err(error) => {
            // A changed CLI definition must retire the old receipt even when
            // its new output cannot be used by this picker.
            let id = selected(settings, target, session).unwrap_or("missing");
            crate::project::activate_context(
                app,
                format!("unavailable:{}:{id}", target.binding.as_deref().unwrap_or("session")),
                None,
                None,
            );
            return Err(error);
        }
    };
    let (key, path, output) = if let Some(project) = &active {
        (
            project.context_key.clone(),
            project.bibliography.clone(),
            Some(project.output),
        )
    } else {
        // Kept native: the path is solely part of the default context identity.
        (
            format!(
                "default:{}:{:?}",
                target.binding.as_deref().unwrap_or("session"),
                settings.project_bibliography
            ),
            settings.project_bibliography.clone(),
            None,
        )
    };
    crate::project::activate_context(app, key, path, output);
    Ok(active)
}

/// Output, edit authority and preview authority from one immutable native
/// observation. Returning them separately could pair project A with B's token.
pub(crate) fn destination_context(
    app: &AppHandle,
    captured: Option<&AppId>,
) -> Result<(Option<ActiveProject>, String, (u32, u32)), String> {
    let target = target(app, captured);
    // An explicit editor request neither reads nor changes GUI project state.
    if target.editor {
        let version = app.state::<crate::project::ProjectBibliography>().version();
        return Ok((None, context_token_for(&target, version.0), version));
    }
    let runtime = app.state::<Runtime>();
    let session = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    if current_target(app) != target {
        return Err(REOPEN.to_owned());
    }
    let settings = load_settings(app, &target)?;
    let active = activate(app, &settings, &target, session.as_deref())?;
    let version = app.state::<crate::project::ProjectBibliography>().version();
    if current_target(app) != target {
        return Err(REOPEN.to_owned());
    }
    Ok((active, context_token_for(&target, version.0), version))
}

pub(crate) fn refresh(app: &AppHandle, captured: Option<&AppId>) -> Result<Option<ActiveProject>, String> {
    destination_context(app, captured).map(|(active, _, _)| active)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn writing_projects(app: AppHandle) -> Result<WritingProjectsView, String> {
    let target = current_target(&app);
    let runtime = app.state::<Runtime>();
    let session = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    if current_target(&app) != target {
        return Err(REOPEN.to_owned());
    }
    let settings = load_settings(&app, &target)?;
    // The manager must still list a selected project with a CLI-only output,
    // so the person can choose a supported output or remove it.
    let _ = activate(&app, &settings, &target, session.as_deref());
    let mut view = view_for(&settings, &target, session.as_deref());
    view.context = context_token(&app, &target);
    Ok(view)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn edit_writing_project(
    app: AppHandle,
    context: String,
    action: WritingProjectEdit,
) -> Result<WritingProjectsView, String> {
    let target = current_target(&app);
    if context_token(&app, &target) != context {
        return Err(REOPEN.to_owned());
    }
    let runtime = app.state::<Runtime>();
    let mut session = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    let (settings, replacement) = app
        .state::<SettingsFile>()
        .exclusive(|path| {
            settings::transaction(path, |settings| {
                // Recheck after acquiring both the process and cross-process writers.
                if current_target(&app) != target || context_token(&app, &target) != context {
                    return Err(ChangeError::Destination);
                }
                edit(settings, &target, session.as_deref(), &action)
            })
        })?
        .map_err(|error| error.to_string())?;
    let current = current_target(&app);
    if current != target {
        // The saved binding still belongs to A. Never publish A's activation
        // or runtime-only selection into a picker that has reopened for B.
        // Removal still retires a runtime choice of that deleted definition.
        if session
            .as_deref()
            .is_some_and(|id| projects::find(&settings, id).is_err())
        {
            *session = None;
        }
        let _ = activate(&app, &settings, &current, session.as_deref());
        let _ = app.emit("writing-projects:changed", ());
        return Err(REOPEN.to_owned());
    }
    *session = replacement;
    let _ = activate(&app, &settings, &target, session.as_deref());
    let mut view = view_for(&settings, &target, session.as_deref());
    view.context = context_token(&app, &target);
    let _ = app.emit("writing-projects:changed", ());
    Ok(view)
}

/// Immutable owner captured before opening a bibliography chooser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BibliographyOwner {
    target: Target,
    id: Option<String>,
    revision: Option<String>,
    default_path: Option<PathBuf>,
    activation: u32,
}

fn owner_for(
    settings: &Settings,
    target: &Target,
    session: Option<&str>,
    activation: u32,
) -> Result<BibliographyOwner, ChangeError> {
    if target.editor {
        return Err(ChangeError::Editor);
    }
    let project = selected(settings, target, session)
        .map(|id| projects::find(settings, id))
        .transpose()?;
    Ok(BibliographyOwner {
        target: target.clone(),
        id: project.map(|project| project.id.clone()),
        revision: project.map(WritingProject::revision),
        default_path: if project.is_none() {
            settings.project_bibliography.clone()
        } else {
            None
        },
        activation,
    })
}

pub(crate) fn bibliography_owner(app: &AppHandle) -> Result<BibliographyOwner, String> {
    let target = current_target(app);
    let runtime = app.state::<Runtime>();
    let session = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    if current_target(app) != target {
        return Err(REOPEN.to_owned());
    }
    let settings = load_settings(app, &target)?;
    let activation = app.state::<crate::project::ProjectBibliography>().version().0;
    owner_for(&settings, &target, session.as_deref(), activation).map_err(|error| error.to_string())
}

pub(crate) fn set_bibliography(
    app: &AppHandle,
    owner: &BibliographyOwner,
    path: Option<PathBuf>,
) -> Result<(), String> {
    let target = current_target(app);
    if target != owner.target {
        return Err(REOPEN.to_owned());
    }
    let runtime = app.state::<Runtime>();
    let session = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    let (settings, ()) = app
        .state::<SettingsFile>()
        .exclusive(|settings_path| {
            settings::transaction(settings_path, |settings| {
                let activation = app.state::<crate::project::ProjectBibliography>().version().0;
                if current_target(app) != target
                    || owner_for(settings, &target, session.as_deref(), activation)? != *owner
                {
                    return Err(ChangeError::Stale);
                }
                if let Some(id) = &owner.id {
                    let output = projects::find(settings, id)?.output.clone();
                    projects::update(settings, id, output, path)?;
                } else {
                    settings.project_bibliography = path;
                }
                Ok::<_, ChangeError>(())
            })
        })?
        .map_err(|error| error.to_string())?;
    let current = current_target(app);
    let activation = activate(app, &settings, &current, session.as_deref());
    let _ = app.emit("writing-projects:changed", ());
    if current != target {
        return Err(REOPEN.to_owned());
    }
    activation.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::cite::KeyFormat;

    fn output() -> AppOutput {
        AppOutput::Key(KeyFormat::Pandoc)
    }
    fn invented(name: &str) -> PathBuf {
        std::env::temp_dir().join("ibis-invented-projects").join(name)
    }
    fn known(name: &str, generation: u64) -> Target {
        Target {
            binding: Some(format!("macos:{name}")),
            generation,
            editor: false,
        }
    }
    fn create(settings: &mut Settings, target: &Target, name: &str) -> String {
        edit(
            settings,
            target,
            None,
            &WritingProjectEdit::Create {
                name: name.into(),
                output: output(),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        settings.writing_project_apps[target.binding.as_ref().unwrap_or_else(|| panic!("an app"))].clone()
    }

    #[test]
    fn switching_projects_changes_output_and_bibliography_together_and_leaving_restores_the_app() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let first = create(&mut settings, &target, "Thesis");
        let revision = projects::find(&settings, &first)
            .unwrap_or_else(|error| panic!("{error}"))
            .revision();
        projects::update(
            &mut settings,
            &first,
            output().project_spec(),
            Some(invented("thesis.bib")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let second = create(&mut settings, &target, "Notes");
        let second_revision = projects::find(&settings, &second)
            .unwrap_or_else(|error| panic!("{error}"))
            .revision();
        edit(
            &mut settings,
            &target,
            None,
            &WritingProjectEdit::Output {
                id: second.clone(),
                revision: second_revision,
                output: AppOutput::Style(ibis_core::settings::CitationStyle::Apa),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(selected(&settings, &target, None), Some(second.as_str()));
        assert_eq!(
            active_for(&settings, &target, None)
                .unwrap_or_else(|error| panic!("{error}"))
                .unwrap_or_else(|| panic!("an active project"))
                .output,
            AppOutput::Style(ibis_core::settings::CitationStyle::Apa)
        );
        edit(
            &mut settings,
            &target,
            None,
            &WritingProjectEdit::Select {
                id: Some(first.clone()),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let active = active_for(&settings, &target, None)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("an active project"));
        assert_eq!(active.output, output());
        assert_eq!(
            projects::find(&settings, &active.id)
                .unwrap_or_else(|error| panic!("{error}"))
                .bibliography,
            Some(invented("thesis.bib"))
        );
        assert_ne!(active.revision, revision);
        edit(&mut settings, &target, None, &WritingProjectEdit::Select { id: None })
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(
            active_for(&settings, &target, None)
                .unwrap_or_else(|error| panic!("{error}"))
                .is_none()
        );
        assert_eq!(settings.writing_projects.len(), 2);
    }

    #[test]
    fn bindings_belong_to_the_captured_app_and_survive_reopening() {
        let mut settings = Settings::default();
        let editor = known("example.editor", 1);
        let browser = known("example.browser", 2);
        let project = create(&mut settings, &editor, "Paper");
        assert_eq!(
            selected(&settings, &known("example.editor", 3), None),
            Some(project.as_str())
        );
        assert_eq!(selected(&settings, &browser, None), None);
        assert_ne!(editor.context(), browser.context());
        assert_ne!(editor.context(), known("example.editor", 3).context());
    }

    #[test]
    fn an_unknown_app_has_only_a_runtime_selection_and_editor_requests_ignore_it() {
        let mut settings = Settings::default();
        let project = create(&mut settings, &known("example.editor", 1), "Paper");
        let unknown = Target {
            binding: None,
            generation: 2,
            editor: false,
        };
        let session = edit(
            &mut settings,
            &unknown,
            None,
            &WritingProjectEdit::Select {
                id: Some(project.clone()),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            selected(&settings, &unknown, session.as_deref()),
            Some(project.as_str())
        );
        assert_eq!(
            selected(&settings, &unknown, None),
            None,
            "a restart has no runtime choice"
        );
        let editor = Target {
            editor: true,
            ..unknown
        };
        assert!(
            active_for(&settings, &editor, session.as_deref())
                .unwrap_or_else(|error| panic!("{error}"))
                .is_none()
        );
        assert!(
            edit(
                &mut settings,
                &editor,
                session.as_deref(),
                &WritingProjectEdit::Select { id: Some(project) }
            )
            .is_err()
        );
        assert_eq!(settings.writing_project_apps.len(), 1);
    }

    #[test]
    fn stale_edits_cannot_overwrite_a_later_definition() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let id = create(&mut settings, &target, "Paper");
        let revision = projects::find(&settings, &id)
            .unwrap_or_else(|error| panic!("{error}"))
            .revision();
        edit(
            &mut settings,
            &target,
            None,
            &WritingProjectEdit::Rename {
                id: id.clone(),
                revision: revision.clone(),
                name: "Final paper".into(),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let saved = serde_json::to_value(&settings).unwrap_or_else(|error| panic!("{error}"));
        for action in [
            WritingProjectEdit::Rename {
                id: id.clone(),
                revision: revision.clone(),
                name: "Old paper".into(),
            },
            WritingProjectEdit::Remove {
                id: id.clone(),
                revision: revision.clone(),
            },
            WritingProjectEdit::Output {
                id: id.clone(),
                revision,
                output: AppOutput::Key(KeyFormat::Cite),
            },
        ] {
            assert!(edit(&mut settings, &target, None, &action).is_err());
            assert_eq!(
                serde_json::to_value(&settings).unwrap_or_else(|error| panic!("{error}")),
                saved
            );
        }
    }

    #[test]
    fn failed_persistence_leaves_the_selection_and_definition_as_they_were() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let target = known("example.editor", 1);
        let (settings, _) = settings::transaction(&path, |settings| {
            edit(
                settings,
                &target,
                None,
                &WritingProjectEdit::Create {
                    name: "Paper".into(),
                    output: output(),
                },
            )
        })
        .unwrap_or_else(|error| panic!("{error}"));
        let id = selected(&settings, &target, None)
            .unwrap_or_else(|| panic!("a selected project"))
            .to_owned();
        let revision = projects::find(&settings, &id)
            .unwrap_or_else(|error| panic!("{error}"))
            .revision();
        std::fs::remove_file(&path).unwrap_or_else(|error| panic!("{error}"));
        std::fs::create_dir(&path).unwrap_or_else(|error| panic!("{error}"));
        let changed = settings::transaction(&path, |settings| {
            edit(
                settings,
                &target,
                None,
                &WritingProjectEdit::Rename {
                    id,
                    revision,
                    name: "Unsaved".into(),
                },
            )
        });
        assert!(changed.is_err());
        assert_eq!(settings.writing_projects[0].name, "Paper");
    }

    #[test]
    fn project_views_never_reveal_the_bibliography_folder() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let id = create(&mut settings, &target, "Paper");
        projects::update(
            &mut settings,
            &id,
            output().project_spec(),
            Some(invented("private-folder/refs.bib")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let shown =
            serde_json::to_string(&view_for(&settings, &target, None)).unwrap_or_else(|error| panic!("{error}"));
        assert!(shown.contains("refs.bib"));
        assert!(!shown.contains("private-folder"));
        assert!(!shown.contains("/invented"));
    }

    #[test]
    fn a_saved_project_reopens_with_its_complete_output_and_bibliography() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let target = known("example.editor", 1);
        let (settings, ()) = settings::transaction(&path, |settings| {
            let id = create(settings, &target, "Thesis");
            projects::update(settings, &id, output().project_spec(), Some(invented("thesis.bib")))?;
            Ok::<_, ChangeError>(())
        })
        .unwrap_or_else(|error| panic!("{error}"));
        let reopened = settings::load(&path);
        assert!(reopened.problem.is_none());
        let before = active_for(&settings, &target, None).unwrap_or_else(|error| panic!("{error}"));
        let after =
            active_for(&reopened.settings, &known("example.editor", 2), None).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            before.map(|project| project.view()),
            after.map(|project| project.view())
        );
    }

    #[test]
    fn a_delayed_bibliography_choice_cannot_return_after_switching_away_and_back() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let first = create(&mut settings, &target, "Thesis");
        let earlier = owner_for(&settings, &target, None, 1).unwrap_or_else(|error| panic!("{error}"));
        create(&mut settings, &target, "Notes");
        edit(
            &mut settings,
            &target,
            None,
            &WritingProjectEdit::Select { id: Some(first) },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let current = owner_for(&settings, &target, None, 3).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(earlier.id, current.id);
        assert_eq!(earlier.revision, current.revision);
        assert_ne!(earlier, current, "a matching definition never revives an old chooser");
    }

    #[test]
    fn unsupported_output_is_visible_in_the_manager_and_refused_before_selecting() {
        use ibis_core::contract::citation::{KeySyntax, OutputSpec};
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let existing = create(&mut settings, &target, "Thesis");
        let other = projects::create(
            &mut settings,
            "Other workflow".into(),
            OutputSpec::Key {
                syntax: KeySyntax::OrgCite,
                command: None,
            },
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let before = settings.clone();
        assert!(
            edit(
                &mut settings,
                &target,
                None,
                &WritingProjectEdit::Select {
                    id: Some(other.id.clone())
                }
            )
            .is_err()
        );
        assert_eq!(settings, before);
        let shown = view_for(&settings, &target, None);
        assert_eq!(shown.selected.as_deref(), Some(existing.as_str()));
        assert!(
            shown
                .projects
                .iter()
                .any(|project| project.id == other.id && project.output.is_none())
        );
    }

    #[test]
    fn duplicate_creation_keeps_the_existing_context_and_removing_clears_all_its_bindings() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let project = create(&mut settings, &target, "Paper");
        let before = settings.clone();
        assert!(
            edit(
                &mut settings,
                &target,
                None,
                &WritingProjectEdit::Create {
                    name: "paper".into(),
                    output: output()
                }
            )
            .is_err()
        );
        assert_eq!(settings, before);
        let other = known("example.browser", 1);
        edit(
            &mut settings,
            &other,
            None,
            &WritingProjectEdit::Select {
                id: Some(project.clone()),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let revision = projects::find(&settings, &project)
            .unwrap_or_else(|error| panic!("{error}"))
            .revision();
        edit(
            &mut settings,
            &target,
            Some(&project),
            &WritingProjectEdit::Remove {
                id: project.clone(),
                revision,
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(settings.writing_projects.is_empty());
        assert!(settings.writing_project_apps.is_empty());
        assert_eq!(
            settings.paste, before.paste,
            "project selection never grants paste permission"
        );
    }

    #[test]
    fn every_picker_output_round_trips_through_the_shared_project_contract() {
        use ibis_core::settings::CitationStyle;
        let formats = [
            KeyFormat::Pandoc,
            KeyFormat::Autocite,
            KeyFormat::Parencite,
            KeyFormat::Textcite,
            KeyFormat::Cite,
            KeyFormat::Typst,
            KeyFormat::Keys,
        ];
        let styles = [
            CitationStyle::Apa,
            CitationStyle::ChicagoAuthorDate,
            CitationStyle::HarvardCiteThemRight,
            CitationStyle::Ieee,
            CitationStyle::ModernLanguageAssociation,
        ];
        for output in formats
            .into_iter()
            .map(AppOutput::Key)
            .chain(styles.into_iter().map(AppOutput::Style))
        {
            assert_eq!(AppOutput::from_project_spec(&output.project_spec()), Ok(output));
        }
    }

    #[test]
    fn unsupported_project_output_never_becomes_a_plausible_picker_default() {
        use ibis_core::contract::citation::{KeySyntax, OutputSpec};
        for output in [
            OutputSpec::Csl {
                style: "invented-custom-style".into(),
            },
            OutputSpec::Preset {
                name: "invented-preset".into(),
            },
            OutputSpec::Key {
                syntax: KeySyntax::Biblatex,
                command: Some("invented-command".into()),
            },
            OutputSpec::Key {
                syntax: KeySyntax::OrgCite,
                command: None,
            },
        ] {
            assert!(AppOutput::from_project_spec(&output).is_err());
        }
    }

    #[test]
    fn unreadable_project_settings_cannot_silently_choose_default_output() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        std::fs::write(&path, "not settings").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            read_settings(&path),
            Err("Writing projects could not be read. Check Settings and try again.".to_owned())
        );
        std::fs::remove_file(&path).unwrap_or_else(|error| panic!("{error}"));
        std::fs::create_dir(&path).unwrap_or_else(|error| panic!("{error}"));
        assert!(read_settings(&path).is_err());
    }

    #[test]
    fn a_validated_settings_backup_keeps_its_project_instead_of_using_defaults() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let target = known("example.editor", 1);
        let (settings, _) = settings::transaction(&path, |settings| {
            edit(
                settings,
                &target,
                None,
                &WritingProjectEdit::Create {
                    name: "Paper".into(),
                    output: output(),
                },
            )
        })
        .unwrap_or_else(|error| panic!("{error}"));
        std::fs::copy(&path, path.with_extension("json.bak")).unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(&path, "not settings").unwrap_or_else(|error| panic!("{error}"));
        let recovered = read_settings(&path).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(recovered.writing_projects, settings.writing_projects);
        assert_eq!(recovered.writing_project_apps, settings.writing_project_apps);
    }
}
