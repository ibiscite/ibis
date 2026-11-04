//! Explicit writing contexts. A saved definition and the app's selection are
//! committed together; selecting one never grants delivery permission.

use std::collections::BTreeMap;
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

/// A reusable saved project is distinct from each ongoing app's task.
#[derive(Default)]
pub(crate) struct Runtime(Mutex<Tasks>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct WritingProjectUpdate {
    pub(crate) name: String,
    pub(crate) output: Option<AppOutput>,
    pub(crate) bibliography: Option<String>,
    pub(crate) revision: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[specta(optional)]
    pub(crate) bibliography_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct WritingProjectView {
    pub(crate) id: String,
    pub(crate) name: String,
    /// A CLI project can name outputs not yet available in the picker.
    pub(crate) output: Option<AppOutput>,
    pub(crate) bibliography: Option<String>,
    pub(crate) revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(crate) pending: Option<WritingProjectUpdate>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[specta(optional)]
    pub(crate) retained: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub(crate) saved_problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub(crate) struct WritingProjectsView {
    pub(crate) projects: Vec<WritingProjectView>,
    pub(crate) selected: Option<String>,
    #[specta(optional)]
    pub(crate) active: Option<WritingProjectView>,
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
    Apply {
        id: String,
        revision: String,
    },
}

#[derive(Debug)]
enum ChangeError {
    Project(ProjectError),
    Stale,
    Destination,
    Editor,
    Unsupported,
    Conflict(String),
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
            Self::Conflict(message) => formatter.write_str(message),
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

fn selected<'a>(settings: &'a Settings, target: &Target, session: Option<&'a str>) -> Option<&'a str> {
    if target.editor {
        return None;
    }
    target.binding.as_ref().map_or(session, |key| {
        settings.writing_project_apps.get(key).map(String::as_str)
    })
}

fn saved_view(project: &WritingProject) -> WritingProjectView {
    WritingProjectView {
        id: project.id.clone(),
        name: project.name.clone(),
        output: AppOutput::from_project_spec(&project.output).ok(),
        bibliography: project.bibliography.as_deref().map(ibis_engine::project_name),
        revision: project.revision(),
        pending: None,
        retained: false,
        saved_problem: None,
    }
}

fn update_view(project: &WritingProject, applied: Option<&WritingProject>) -> WritingProjectUpdate {
    WritingProjectUpdate {
        name: project.name.clone(),
        output: AppOutput::from_project_spec(&project.output).ok(),
        bibliography: project.bibliography.as_deref().map(ibis_engine::project_name),
        revision: project.revision(),
        bibliography_changed: applied.is_some_and(|applied| applied.bibliography != project.bibliography),
    }
}

fn same_choices(a: &WritingProject, b: &WritingProject) -> bool {
    a.output == b.output && a.bibliography == b.bibliography
}

#[derive(Clone, Debug)]
struct Task {
    project: Option<WritingProject>,
    pending: Option<WritingProject>,
    retained: bool,
    saved_problem: Option<String>,
    epoch: u64,
}

impl Task {
    fn reconcile(&mut self, settings: &Settings) {
        self.saved_problem = None;
        let Some(applied) = self.project.as_mut() else { return };
        let Ok(saved) = projects::find(settings, &applied.id) else {
            self.retained = true;
            self.pending = None;
            return;
        };
        self.retained = false;
        if same_choices(applied, saved) {
            // A name is cosmetic. Adopt its edit identity as well so a later
            // save does not spuriously conflict with a rename already shown.
            *applied = saved.clone();
            self.pending = None;
        } else {
            applied.name = saved.name.clone();
            self.pending = Some(saved.clone());
        }
    }

    fn view(&self) -> Option<WritingProjectView> {
        self.project.as_ref().map(|project| {
            let mut view = saved_view(project);
            view.pending = self
                .pending
                .as_ref()
                .map(|saved| update_view(saved, self.project.as_ref()));
            view.retained = self.retained;
            view.saved_problem = self.saved_problem.clone();
            view
        })
    }

    fn active(&self, target: &Target) -> Result<Option<ActiveProject>, String> {
        let Some(project) = &self.project else { return Ok(None) };
        Ok(Some(ActiveProject {
            id: project.id.clone(),
            name: project.name.clone(),
            output: AppOutput::from_project_spec(&project.output)?,
            bibliography: project.bibliography.clone(),
            revision: project.revision(),
            context_key: format!("task:{}:{}", target.binding.as_deref().unwrap_or("session"), self.epoch),
            pending: self
                .pending
                .as_ref()
                .map(|saved| update_view(saved, self.project.as_ref())),
            retained: self.retained,
            saved_problem: self.saved_problem.clone(),
        }))
    }
}

#[derive(Default)]
struct Tasks {
    tasks: BTreeMap<Option<String>, Task>,
    next: u64,
}

impl Tasks {
    fn preflight(&self, target: &Target, edits: u64) -> Result<(), String> {
        if !self.tasks.contains_key(&target.binding)
            && target.binding.is_some()
            && self.tasks.keys().filter(|key| key.is_some()).count() >= 200
        {
            return Err(
                "Ibis already has tasks for 200 apps. Restart Ibis before starting another app's task.".to_owned(),
            );
        }
        self.next.checked_add(edits).ok_or_else(|| CHANGED.to_owned())?;
        Ok(())
    }

    fn preflight_edit(&self, target: &Target) -> Result<(), String> {
        self.preflight(target, if self.tasks.contains_key(&target.binding) { 1 } else { 2 })
    }

    fn task(&mut self, settings: &Settings, target: &Target) -> Result<&mut Task, String> {
        if !self.tasks.contains_key(&target.binding) {
            let id = selected(settings, target, None);
            self.adopt(settings, target, id)?;
        }
        let task = self.tasks.get_mut(&target.binding).ok_or_else(|| CHANGED.to_owned())?;
        task.reconcile(settings);
        Ok(task)
    }

    fn adopt(&mut self, settings: &Settings, target: &Target, id: Option<&str>) -> Result<(), String> {
        self.preflight(target, 1)?;
        let project = id
            .map(|id| projects::find(settings, id).cloned())
            .transpose()
            .map_err(|error| error.to_string())?;
        self.next = self.next.checked_add(1).ok_or_else(|| CHANGED.to_owned())?;
        self.tasks.insert(
            target.binding.clone(),
            Task {
                project,
                pending: None,
                retained: false,
                saved_problem: None,
                epoch: self.next,
            },
        );
        Ok(())
    }

    fn existing(&mut self, target: &Target) -> Option<&mut Task> {
        self.tasks.get_mut(&target.binding)
    }
}

fn view_for(settings: &Settings, target: &Target, session: Option<&str>) -> WritingProjectsView {
    WritingProjectsView {
        projects: settings.writing_projects.iter().map(saved_view).collect(),
        selected: selected(settings, target, session).map(str::to_owned),
        active: None,
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
    pub(crate) pending: Option<WritingProjectUpdate>,
    pub(crate) retained: bool,
    pub(crate) saved_problem: Option<String>,
}

impl ActiveProject {
    pub(crate) fn view(&self) -> WritingProjectView {
        WritingProjectView {
            id: self.id.clone(),
            name: self.name.clone(),
            output: Some(self.output),
            bibliography: self.bibliography.as_deref().map(ibis_engine::project_name),
            revision: self.revision.clone(),
            pending: self.pending.clone(),
            retained: self.retained,
            saved_problem: self.saved_problem.clone(),
        }
    }
}

#[cfg(test)]
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
        pending: None,
        retained: false,
        saved_problem: None,
    }))
}

fn checked<'a>(settings: &'a Settings, id: &str, revision: &str) -> Result<&'a WritingProject, ChangeError> {
    let project = projects::find(settings, id)?;
    if project.revision() != revision {
        return Err(ChangeError::Conflict(format!(
            "The writing project changed elsewhere. Saved choices: {} · {}. Apply Updates before saving your changes.",
            output_label(AppOutput::from_project_spec(&project.output).ok()),
            project
                .bibliography
                .as_deref()
                .map_or_else(|| "no bibliography".to_owned(), ibis_engine::project_name),
        )));
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
        WritingProjectEdit::Apply { id, revision } => {
            let project = checked(settings, id, revision)?;
            AppOutput::from_project_spec(&project.output).map_err(|_| ChangeError::Unsupported)?;
        }
    }
    Ok(session)
}

fn output_label(output: Option<AppOutput>) -> &'static str {
    use ibis_core::cite::KeyFormat;
    use ibis_core::settings::CitationStyle;
    match output {
        Some(AppOutput::Key(KeyFormat::Pandoc)) => "Pandoc keys",
        Some(AppOutput::Key(KeyFormat::Autocite)) => "BibLaTeX autocite",
        Some(AppOutput::Key(KeyFormat::Parencite)) => "BibLaTeX parencite",
        Some(AppOutput::Key(KeyFormat::Textcite)) => "BibLaTeX textcite",
        Some(AppOutput::Key(KeyFormat::Cite)) => "LaTeX keys",
        Some(AppOutput::Key(KeyFormat::Typst)) => "Typst keys",
        Some(AppOutput::Key(KeyFormat::Keys)) => "citation keys",
        Some(AppOutput::Style(CitationStyle::Apa)) => "APA",
        Some(AppOutput::Style(CitationStyle::ChicagoAuthorDate)) => "Chicago",
        Some(AppOutput::Style(CitationStyle::HarvardCiteThemRight)) => "Harvard",
        Some(AppOutput::Style(CitationStyle::Ieee)) => "IEEE",
        Some(AppOutput::Style(CitationStyle::ModernLanguageAssociation)) => "MLA",
        None => "an output available in the command line",
    }
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

fn task_view(settings: Option<&Settings>, target: &Target, task: &Task) -> WritingProjectsView {
    let mut view = settings.map_or_else(
        || WritingProjectsView {
            projects: Vec::new(),
            selected: None,
            active: None,
            session_only: target.binding.is_none(),
            context: target.context(),
        },
        |settings| view_for(settings, target, None),
    );
    view.active = task.view();
    view.selected = view.active.as_ref().map(|project| project.id.clone());
    view
}

impl Tasks {
    fn observe(&mut self, settings: Result<Settings, String>, target: &Target) -> Result<Option<Settings>, String> {
        match settings {
            Ok(settings) => {
                self.task(&settings, target)?;
                Ok(Some(settings))
            }
            Err(error) => {
                let task = self
                    .existing(target)
                    .filter(|task| task.project.is_some())
                    .ok_or(error)?;
                task.saved_problem = Some("Saved project settings could not be checked. Using these task settings. Check Settings and try again.".to_owned());
                Ok(None)
            }
        }
    }
}

fn observe(app: &AppHandle, target: &Target, tasks: &mut Tasks) -> Result<Option<Settings>, String> {
    let before = tasks.existing(target).and_then(|task| task.view());
    let observed = tasks.observe(read_settings(&app.state::<SettingsFile>().path), target);
    let after = tasks.existing(target).and_then(|task| task.view());
    if before != after {
        // Metadata does not retire citation authority. Publishing only a changed
        // observation also prevents the listener's read from emitting a loop.
        let _ = app.emit("writing-projects:changed", ());
    }
    if observed.is_err() {
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
    observed
}

fn activate_task(
    app: &AppHandle,
    settings: Option<&Settings>,
    target: &Target,
    task: &Task,
) -> Result<Option<ActiveProject>, String> {
    if target.editor {
        return Ok(None);
    }
    let active = match task.active(target) {
        Ok(active) => active,
        Err(error) => {
            crate::project::activate_context(
                app,
                format!(
                    "unavailable:task:{}:{}",
                    target.binding.as_deref().unwrap_or("session"),
                    task.epoch
                ),
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
        let settings = settings.ok_or_else(|| CHANGED.to_owned())?;
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

/// Output, edit authority and preview authority from one immutable native observation.
pub(crate) struct DestinationContext {
    pub(crate) active: Option<ActiveProject>,
    pub(crate) edit: String,
    pub(crate) version: (u32, u32),
}

pub(crate) fn destination_context(app: &AppHandle, captured: Option<&AppId>) -> Result<DestinationContext, String> {
    let target = target(app, captured);
    if target.editor {
        let version = app.state::<crate::project::ProjectBibliography>().version();
        return Ok(DestinationContext {
            active: None,
            edit: context_token_for(&target, version.0),
            version,
        });
    }
    let runtime = app.state::<Runtime>();
    let mut tasks = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    if current_target(app) != target {
        return Err(REOPEN.to_owned());
    }
    let settings = observe(app, &target, &mut tasks)?;
    let task = tasks.existing(&target).ok_or_else(|| CHANGED.to_owned())?;
    let active = activate_task(app, settings.as_ref(), &target, task)?;
    let version = app.state::<crate::project::ProjectBibliography>().version();
    if current_target(app) != target {
        return Err(REOPEN.to_owned());
    }
    Ok(DestinationContext {
        active,
        edit: context_token_for(&target, version.0),
        version,
    })
}

pub(crate) fn refresh(app: &AppHandle, captured: Option<&AppId>) -> Result<Option<ActiveProject>, String> {
    destination_context(app, captured).map(|context| context.active)
}

#[tauri::command]
#[specta::specta]
pub(crate) fn writing_projects(app: AppHandle) -> Result<WritingProjectsView, String> {
    let target = current_target(&app);
    let runtime = app.state::<Runtime>();
    let mut tasks = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    if current_target(&app) != target {
        return Err(REOPEN.to_owned());
    }
    if target.editor {
        return Err(EDITOR.to_owned());
    }
    let settings = observe(&app, &target, &mut tasks)?;
    let task = tasks.existing(&target).ok_or_else(|| CHANGED.to_owned())?;
    // List an unsupported initial project so its definition can be repaired.
    let _ = activate_task(&app, settings.as_ref(), &target, task);
    let mut view = task_view(settings.as_ref(), &target, task);
    view.context = context_token(&app, &target);
    Ok(view)
}

fn effective_action(settings: &Settings, task: &Task, action: &WritingProjectEdit) -> WritingProjectEdit {
    let mut action = action.clone();
    let (id, revision) = match &mut action {
        WritingProjectEdit::Output { id, revision, .. } => (id, revision),
        _ => return action,
    };
    if let Some(applied) = task.project.as_ref().filter(|project| project.id == *id)
        && let Ok(saved) = projects::find(settings, id)
        && same_choices(applied, saved)
    {
        // Output edits preserve the newest name. Name edits themselves still
        // conflict so one writer cannot silently erase another writer's name.
        *revision = saved.revision();
    }
    action
}

fn apply_local(
    tasks: &mut Tasks,
    settings: &Settings,
    target: &Target,
    action: &WritingProjectEdit,
    replacement: Option<String>,
) -> Result<(), String> {
    let current = tasks
        .existing(target)
        .and_then(|task| task.project.as_ref().map(|project| project.id.clone()));
    match action {
        WritingProjectEdit::Create { .. } | WritingProjectEdit::Select { .. } => {
            let id = if target.binding.is_some() {
                selected(settings, target, None).map(str::to_owned)
            } else {
                replacement
            };
            tasks.adopt(settings, target, id.as_deref())
        }
        WritingProjectEdit::Apply { id, .. } => {
            if current.as_deref() != Some(id) {
                return Err(CHANGED.to_owned());
            }
            tasks.adopt(settings, target, Some(id))
        }
        WritingProjectEdit::Output { id, .. } if current.as_deref() == Some(id) => {
            tasks.adopt(settings, target, Some(id))
        }
        WritingProjectEdit::Remove { id, .. } if current.as_deref() == Some(id) => tasks.adopt(settings, target, None),
        _ => {
            tasks.task(settings, target)?;
            Ok(())
        }
    }
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
    if target.editor {
        return Err(EDITOR.to_owned());
    }
    let runtime = app.state::<Runtime>();
    let mut tasks = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    tasks.preflight_edit(&target)?;
    let before = tasks.existing(&target).and_then(|task| task.view());
    let (settings, replacement) = app
        .state::<SettingsFile>()
        .exclusive(|path| {
            settings::transaction(path, |settings| {
                if current_target(&app) != target || context_token(&app, &target) != context {
                    return Err(ChangeError::Destination);
                }
                // The original task is immutable until persistence succeeds.
                let task = tasks.task(settings, &target).map_err(|_| ChangeError::Stale)?.clone();
                if let WritingProjectEdit::Apply { id, .. } = &action
                    && task.project.as_ref().map(|project| project.id.as_str()) != Some(id.as_str())
                {
                    return Err(ChangeError::Stale);
                }
                let effective = effective_action(settings, &task, &action);
                let session = if target.binding.is_none() {
                    task.project.as_ref().map(|project| project.id.as_str())
                } else {
                    None
                };
                edit(settings, &target, session, &effective)
            })
        })?
        .map_err(|error| {
            if before != tasks.existing(&target).and_then(|task| task.view()) {
                let _ = app.emit("writing-projects:changed", ());
            }
            error.to_string()
        })?;
    // Save only changed the initiating app's task. Another app's task retains its own snapshot.
    apply_local(&mut tasks, &settings, &target, &action, replacement)?;
    let current = current_target(&app);
    if current != target {
        let task = tasks.task(&settings, &current)?;
        let _ = activate_task(&app, Some(&settings), &current, task);
        let _ = app.emit("writing-projects:changed", ());
        return Err(REOPEN.to_owned());
    }
    let task = tasks.existing(&target).ok_or_else(|| CHANGED.to_owned())?;
    let _ = activate_task(&app, Some(&settings), &target, task);
    let mut view = task_view(Some(&settings), &target, task);
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

#[cfg(test)]
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

fn task_owner(
    settings: &Settings,
    target: &Target,
    task: &Task,
    activation: u32,
) -> Result<BibliographyOwner, ChangeError> {
    if target.editor {
        return Err(ChangeError::Editor);
    }
    Ok(BibliographyOwner {
        target: target.clone(),
        id: task.project.as_ref().map(|project| project.id.clone()),
        revision: task.project.as_ref().map(WritingProject::revision),
        default_path: if task.project.is_none() {
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
    let mut tasks = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    if current_target(app) != target {
        return Err(REOPEN.to_owned());
    }
    let settings = observe(app, &target, &mut tasks)?.ok_or_else(|| {
        "Saved project settings could not be read. Check Settings before editing the bibliography.".to_owned()
    })?;
    let task = tasks.existing(&target).ok_or_else(|| CHANGED.to_owned())?;
    let activation = app.state::<crate::project::ProjectBibliography>().version().0;
    task_owner(&settings, &target, task, activation).map_err(|error| error.to_string())
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
    let mut tasks = runtime.0.lock().map_err(|_| CHANGED.to_owned())?;
    tasks.preflight_edit(&target)?;
    let before = tasks.existing(&target).and_then(|task| task.view());
    let (settings, ()) = app
        .state::<SettingsFile>()
        .exclusive(|settings_path| {
            settings::transaction(settings_path, |settings| {
                let activation = app.state::<crate::project::ProjectBibliography>().version().0;
                let task = tasks.task(settings, &target).map_err(|_| ChangeError::Stale)?;
                if current_target(app) != target || task_owner(settings, &target, task, activation)? != *owner {
                    return Err(ChangeError::Stale);
                }
                if let Some(id) = &owner.id {
                    let revision = owner.revision.as_deref().ok_or(ChangeError::Stale)?;
                    let output = checked(settings, id, revision)?.output.clone();
                    projects::update(settings, id, output, path)?;
                } else {
                    settings.project_bibliography = path;
                }
                Ok::<_, ChangeError>(())
            })
        })?
        .map_err(|error| {
            if before != tasks.existing(&target).and_then(|task| task.view()) {
                let _ = app.emit("writing-projects:changed", ());
            }
            error.to_string()
        })?;
    if let Some(id) = &owner.id {
        tasks.adopt(&settings, &target, Some(id))?;
    }
    let current = current_target(app);
    let task = tasks.task(&settings, &current)?;
    let activation = activate_task(app, Some(&settings), &current, task);
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

    #[test]
    fn a_shared_material_edit_is_pending_across_citations_and_reopening_until_applied() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let id = create(&mut settings, &target, "Thesis");
        let mut runtime = Tasks::default();
        let first = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"));
        let epoch = first.epoch;
        projects::update(
            &mut settings,
            &id,
            AppOutput::Style(ibis_core::settings::CitationStyle::Apa).project_spec(),
            Some(invented("revised.bib")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        for opening in [2, 3] {
            let task = runtime
                .task(&settings, &known("example.editor", opening))
                .unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(task.epoch, epoch);
            let view = task.view().unwrap_or_else(|| panic!("task settings"));
            assert_eq!(view.output, Some(output()));
            assert_eq!(view.bibliography, None);
            assert_eq!(
                view.pending.as_ref().and_then(|update| update.bibliography.as_deref()),
                Some("revised.bib")
            );
        }
        runtime
            .adopt(&settings, &target, Some(&id))
            .unwrap_or_else(|error| panic!("{error}"));
        let task = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_ne!(task.epoch, epoch);
        let view = task.view().unwrap_or_else(|| panic!("task settings"));
        assert_eq!(
            view.output,
            Some(AppOutput::Style(ibis_core::settings::CitationStyle::Apa))
        );
        assert_eq!(view.bibliography.as_deref(), Some("revised.bib"));
        assert!(view.pending.is_none());
    }

    #[test]
    fn a_cosmetic_rename_updates_the_label_without_repreparing_or_hiding_a_material_update() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let id = create(&mut settings, &target, "Thesis");
        let mut runtime = Tasks::default();
        let epoch = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"))
            .epoch;
        projects::rename(&mut settings, &id, "Final thesis".into()).unwrap_or_else(|error| panic!("{error}"));
        let task = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"));
        let view = task.view().unwrap_or_else(|| panic!("task settings"));
        assert_eq!(view.name, "Final thesis");
        assert_eq!(
            view.revision,
            projects::find(&settings, &id)
                .unwrap_or_else(|error| panic!("{error}"))
                .revision()
        );
        assert!(view.pending.is_none());
        assert_eq!(task.epoch, epoch);
        projects::update(
            &mut settings,
            &id,
            AppOutput::Key(KeyFormat::Cite).project_spec(),
            Some(invented("paper.bib")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        projects::rename(&mut settings, &id, "Submitted thesis".into()).unwrap_or_else(|error| panic!("{error}"));
        let task = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"));
        let view = task.view().unwrap_or_else(|| panic!("task settings"));
        assert_eq!(view.name, "Submitted thesis");
        assert_eq!(view.output, Some(output()));
        assert!(view.pending.is_some());
        assert_eq!(task.epoch, epoch);
    }

    #[test]
    fn deleting_a_preset_elsewhere_retains_its_task_choices_and_explicit_selection_starts_fresh() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let first = create(&mut settings, &target, "Thesis");
        projects::update(
            &mut settings,
            &first,
            output().project_spec(),
            Some(invented("thesis.bib")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let mut runtime = Tasks::default();
        let epoch = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"))
            .epoch;
        projects::remove(&mut settings, &first).unwrap_or_else(|error| panic!("{error}"));
        let other = create(&mut settings, &target, "Notes");
        let task = runtime
            .task(&settings, &known("example.editor", 2))
            .unwrap_or_else(|error| panic!("{error}"));
        let view = task.view().unwrap_or_else(|| panic!("retained task settings"));
        assert_eq!(view.id, first);
        assert!(view.retained);
        assert_eq!(view.bibliography.as_deref(), Some("thesis.bib"));
        assert_eq!(task.epoch, epoch);
        runtime
            .adopt(&settings, &target, Some(&other))
            .unwrap_or_else(|error| panic!("{error}"));
        let view = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"))
            .view()
            .unwrap_or_else(|| panic!("new task"));
        assert_eq!(view.id, other);
        assert!(!view.retained);
        assert_eq!(view.bibliography, None);
    }

    #[test]
    fn tasks_for_two_apps_keep_their_project_choices_when_shared_bindings_change() {
        let mut settings = Settings::default();
        let a = known("example.editor", 1);
        let b = known("example.browser", 2);
        let first = create(&mut settings, &a, "Thesis");
        let second = create(&mut settings, &b, "Notes");
        let mut runtime = Tasks::default();
        let a_epoch = runtime
            .task(&settings, &a)
            .unwrap_or_else(|error| panic!("{error}"))
            .epoch;
        let b_epoch = runtime
            .task(&settings, &b)
            .unwrap_or_else(|error| panic!("{error}"))
            .epoch;
        edit(
            &mut settings,
            &a,
            None,
            &WritingProjectEdit::Select {
                id: Some(second.clone()),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        edit(
            &mut settings,
            &b,
            None,
            &WritingProjectEdit::Select {
                id: Some(first.clone()),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let a_task = runtime
            .task(&settings, &known("example.editor", 3))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(a_task.view().unwrap_or_else(|| panic!("A task")).id, first);
        assert_eq!(a_task.epoch, a_epoch);
        let b_task = runtime
            .task(&settings, &known("example.browser", 4))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(b_task.view().unwrap_or_else(|| panic!("B task")).id, second);
        assert_eq!(b_task.epoch, b_epoch);
    }

    #[test]
    fn shared_settings_read_failure_preserves_only_an_established_task_and_blocks_edits() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        create(&mut settings, &target, "Thesis");
        let mut runtime = Tasks::default();
        let before = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"))
            .clone();
        std::fs::write(&path, "invalid settings").unwrap_or_else(|error| panic!("{error}"));
        assert!(
            runtime
                .observe(read_settings(&path), &known("example.editor", 2))
                .unwrap_or_else(|error| panic!("{error}"))
                .is_none()
        );
        let task = runtime.existing(&target).unwrap_or_else(|| panic!("existing task"));
        assert_eq!(task.epoch, before.epoch);
        assert_eq!(task.project, before.project);
        assert!(
            task.view()
                .unwrap_or_else(|| panic!("task settings"))
                .saved_problem
                .is_some()
        );
        assert!(
            runtime
                .observe(read_settings(&path), &known("example.browser", 3))
                .is_err()
        );
        let saved = settings::transaction(&path, |settings| {
            edit(
                settings,
                &target,
                None,
                &WritingProjectEdit::Create {
                    name: "Unsaved".into(),
                    output: output(),
                },
            )
        });
        assert!(saved.is_err());
        assert_eq!(
            runtime
                .existing(&target)
                .unwrap_or_else(|| panic!("existing task"))
                .project,
            before.project
        );
        runtime
            .observe(Ok(settings), &target)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(
            runtime
                .existing(&target)
                .unwrap_or_else(|| panic!("existing task"))
                .saved_problem
                .is_none()
        );
    }

    #[test]
    fn conflicting_save_preserves_task_choices_and_describes_the_saved_changes() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let id = create(&mut settings, &target, "Thesis");
        let mut runtime = Tasks::default();
        let before = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"))
            .clone();
        let revision = before
            .project
            .as_ref()
            .unwrap_or_else(|| panic!("a project"))
            .revision();
        projects::update(
            &mut settings,
            &id,
            AppOutput::Style(ibis_core::settings::CitationStyle::Apa).project_spec(),
            Some(invented("revised.bib")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let task = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"));
        let action = WritingProjectEdit::Output {
            id,
            revision,
            output: AppOutput::Key(KeyFormat::Cite),
        };
        let effective = effective_action(&settings, task, &action);
        let error = edit(&mut settings, &target, None, &effective)
            .err()
            .unwrap_or_else(|| panic!("a conflicting save"))
            .to_string();
        assert!(error.contains("APA"));
        assert!(error.contains("revised.bib"));
        assert!(!error.contains("ibis-invented-projects"));
        assert_eq!(task.epoch, before.epoch);
        assert_eq!(task.project, before.project);
        assert!(task.pending.is_some());
    }

    #[test]
    fn a_current_task_output_save_reprepares_only_after_saving_and_keeps_an_external_cosmetic_name() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let id = create(&mut settings, &target, "Thesis");
        let mut runtime = Tasks::default();
        let before = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"))
            .clone();
        let action = WritingProjectEdit::Output {
            id: id.clone(),
            revision: before
                .project
                .as_ref()
                .unwrap_or_else(|| panic!("a project"))
                .revision(),
            output: AppOutput::Key(KeyFormat::Cite),
        };
        projects::rename(&mut settings, &id, "Final thesis".into()).unwrap_or_else(|error| panic!("{error}"));
        let task = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"));
        let effective = effective_action(&settings, task, &action);
        assert_eq!(task.epoch, before.epoch);
        assert_eq!(
            task.view().unwrap_or_else(|| panic!("task settings")).output,
            Some(output())
        );
        let replacement = edit(&mut settings, &target, None, &effective).unwrap_or_else(|error| panic!("{error}"));
        apply_local(&mut runtime, &settings, &target, &action, replacement).unwrap_or_else(|error| panic!("{error}"));
        let task = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_ne!(task.epoch, before.epoch);
        assert_eq!(
            task.view().unwrap_or_else(|| panic!("task settings")).output,
            Some(AppOutput::Key(KeyFormat::Cite))
        );
        assert_eq!(
            task.view().unwrap_or_else(|| panic!("task settings")).name,
            "Final thesis"
        );
    }

    #[test]
    fn the_task_limit_refuses_a_new_app_without_evicting_an_established_task() {
        let mut settings = Settings::default();
        let first = known("example.editor0", 1);
        let id = create(&mut settings, &first, "Thesis");
        let mut runtime = Tasks::default();
        for index in 0..200 {
            runtime
                .task(&settings, &known(&format!("example.editor{index}"), 1))
                .unwrap_or_else(|error| panic!("{error}"));
        }
        let epoch = runtime.existing(&first).unwrap_or_else(|| panic!("first task")).epoch;
        assert!(runtime.preflight_edit(&known("example.overflow", 2)).is_err());
        let unknown = Target {
            binding: None,
            generation: 3,
            editor: false,
        };
        runtime
            .task(&settings, &unknown)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(runtime.tasks.len(), 201);
        assert!(runtime.task(&settings, &known("example.overflow", 4)).is_err());
        let task = runtime
            .task(&settings, &known("example.editor0", 5))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(task.epoch, epoch);
        assert_eq!(task.view().unwrap_or_else(|| panic!("first task settings")).id, id);
    }

    #[test]
    fn task_epoch_exhaustion_is_refused_before_a_shared_save_can_start() {
        let settings = Settings::default();
        let target = known("example.editor", 1);
        let mut runtime = Tasks {
            next: u64::MAX - 1,
            ..Tasks::default()
        };
        assert!(runtime.preflight_edit(&target).is_err());
        assert!(runtime.tasks.is_empty());
        runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(runtime.next, u64::MAX);
        assert!(runtime.preflight_edit(&target).is_err());
        assert_eq!(runtime.tasks.len(), 1);
    }

    #[test]
    fn a_changed_bibliography_location_is_visible_even_when_its_name_is_the_same() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let id = create(&mut settings, &target, "Thesis");
        projects::update(
            &mut settings,
            &id,
            output().project_spec(),
            Some(invented("first-private-folder/refs.bib")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let mut runtime = Tasks::default();
        runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"));
        projects::update(
            &mut settings,
            &id,
            output().project_spec(),
            Some(invented("second-private-folder/refs.bib")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let view = runtime
            .task(&settings, &target)
            .unwrap_or_else(|error| panic!("{error}"))
            .view()
            .unwrap_or_else(|| panic!("task settings"));
        assert_eq!(view.bibliography.as_deref(), Some("refs.bib"));
        let pending = view.pending.as_ref().unwrap_or_else(|| panic!("pending settings"));
        assert_eq!(pending.bibliography.as_deref(), Some("refs.bib"));
        assert!(pending.bibliography_changed);
        let serialized = serde_json::to_string(&view).unwrap_or_else(|error| panic!("{error}"));
        assert!(!serialized.contains("first-private-folder"));
        assert!(!serialized.contains("second-private-folder"));
    }
}
