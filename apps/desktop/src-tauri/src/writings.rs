//! Explicit writing contexts. A saved definition and the app's selection are
//! committed together; selecting one never grants delivery permission. Each
//! opening reads the projects as saved, so a change made in another app is
//! used by the next citation.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ibis_core::destination::{AppId, System};
use ibis_engine::settings::writings::{self as definitions, Writing, WritingError};
use ibis_engine::settings::{self, Settings};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::library::CommandError;
use crate::output::OutputRef;
use crate::preferences::SettingsFile;

const CHANGED: &str = "The writing changed. Open the format menu again.";
const REOPEN: &str = "The destination changed. Open the format menu again.";
const EDITOR: &str = "This editor request chooses its own output. Finish it before changing writings.";

/// The session-only project of an app Ibis cannot identify; never saved.
#[derive(Default)]
pub(crate) struct Runtime(Mutex<Option<String>>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub(crate) struct WritingView {
    pub(crate) id: String,
    pub(crate) name: String,
    /// A writing can name outputs not yet available in the picker.
    #[schemars(with = "Option<String>")]
    pub(crate) output: Option<OutputRef>,
    pub(crate) bibliography: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub(crate) struct WritingsView {
    pub(crate) writings: Vec<WritingView>,
    pub(crate) selected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) active: Option<WritingView>,
    pub(crate) session_only: bool,
    /// Owns one picker opening and captured destination; never a path or title.
    pub(crate) context: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum WritingEdit {
    Create {
        name: String,
        #[schemars(with = "String")]
        output: OutputRef,
    },
    Rename {
        id: String,
        name: String,
    },
    Remove {
        id: String,
    },
    Select {
        id: Option<String>,
    },
    Output {
        id: String,
        #[schemars(with = "String")]
        output: OutputRef,
    },
}

#[derive(Debug)]
enum ChangeError {
    Project(WritingError),
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
                formatter.write_str("This writing's format is not available in the picker. Choose another format.")
            }
        }
    }
}
impl From<WritingError> for ChangeError {
    fn from(error: WritingError) -> Self {
        Self::Project(error)
    }
}
impl From<settings::SettingsError> for ChangeError {
    fn from(error: settings::SettingsError) -> Self {
        Self::Project(WritingError::Settings(error))
    }
}

impl From<ChangeError> for CommandError {
    fn from(error: ChangeError) -> Self {
        use ibis_core::contract::{Problem, Recovery as R, RecoveryInfo};
        let (code, id, recovery) = match error {
            ChangeError::Project(error) => return Self::Presented((&error).into()),
            ChangeError::Stale | ChangeError::Destination => {
                ("project-changed", "app-project-error-changed", R::ReopenProjectMenu)
            }
            ChangeError::Editor => ("editor-pending", "app-project-error-editor", R::FinishEditorRequest),
            ChangeError::Unsupported => ("project-output-unavailable", "app-project-error-output", R::ChooseStyle),
        };
        Self::Presented(Problem {
            code: code.into(),
            info: RecoveryInfo::new(id, recovery),
        })
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
    fn owner(&self) -> &str {
        self.binding.as_deref().unwrap_or("session")
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

fn read_settings(path: &Path) -> Result<Settings, CommandError> {
    settings::load(path).cites().cloned().map_err(CommandError::from)
}

/// The saved settings, or no project at all: an unreadable file never lets
/// an earlier project or the default output stand in for the saved one.
fn saved_settings(app: &AppHandle, target: &Target) -> Result<Settings, CommandError> {
    read_settings(&app.state::<SettingsFile>().path).inspect_err(|_| {
        crate::project::activate_context(app, format!("unavailable:settings:{}", target.owner()), None, None);
    })
}

/// The selected project's ID, if it still exists.
fn selected<'a>(settings: &'a Settings, target: &Target, session: Option<&'a str>) -> Option<&'a str> {
    if target.editor {
        return None;
    }
    target
        .binding
        .as_ref()
        .map_or(session, |key| settings.device.writing_apps.get(key).map(String::as_str))
        .filter(|id| definitions::find(settings, id).is_ok())
}

pub(crate) fn saved_view(project: &Writing) -> WritingView {
    WritingView {
        id: project.name.clone(),
        name: project.name.clone(),
        output: crate::output::of_writing(&project.format).ok(),
        bibliography: project.bibliography.as_deref().map(ibis_engine::project_name),
    }
}

fn view_for(settings: &Settings, target: &Target, session: Option<&str>) -> WritingsView {
    let selected = selected(settings, target, session);
    WritingsView {
        writings: settings.device.writings.iter().map(saved_view).collect(),
        selected: selected.map(str::to_owned),
        active: selected
            .and_then(|id| definitions::find(settings, id).ok())
            .map(saved_view),
        session_only: target.binding.is_none(),
        context: target.context(),
    }
}

/// Native-only context. The bibliography path never enters an IPC view.
#[derive(Debug, Clone)]
pub(crate) struct ActiveWriting {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) output: OutputRef,
    pub(crate) bibliography: Option<PathBuf>,
    pub(crate) context_key: String,
}

impl ActiveWriting {
    pub(crate) fn view(&self) -> WritingView {
        WritingView {
            id: self.id.clone(),
            name: self.name.clone(),
            output: Some(self.output),
            bibliography: self.bibliography.as_deref().map(ibis_engine::project_name),
        }
    }
}

fn active_for(
    settings: &Settings,
    target: &Target,
    session: Option<&str>,
) -> Result<Option<ActiveWriting>, CommandError> {
    let Some(id) = selected(settings, target, session) else {
        return Ok(None);
    };
    let project = definitions::find(settings, id).map_err(ChangeError::from)?;
    Ok(Some(ActiveWriting {
        id: project.name.clone(),
        name: project.name.clone(),
        output: crate::output::of_writing(&project.format).map_err(|_| ChangeError::Unsupported)?,
        bibliography: project.bibliography.clone(),
        // The bibliography and output are compared on their own.
        context_key: format!("project:{}:{}", target.owner(), project.name),
    }))
}

/// Make the saved choice this destination's checking context.
fn activate(
    app: &AppHandle,
    settings: &Settings,
    target: &Target,
    session: Option<&str>,
) -> Result<Option<ActiveWriting>, CommandError> {
    if target.editor {
        return Ok(None);
    }
    let active = active_for(settings, target, session);
    let (key, path, output) = match &active {
        Ok(Some(project)) => (
            project.context_key.clone(),
            project.bibliography.clone(),
            Some(project.output),
        ),
        Ok(None) => {
            let app = target.binding.as_ref().and_then(|binding| {
                let (system, id) = binding.split_once(':')?;
                let system = match system {
                    "macos" => System::Macos,
                    "windows" => System::Windows,
                    "linux" => System::Linux,
                    _ => return None,
                };
                Some(AppId { system, id: id.into() })
            });
            let choice = ibis_engine::format::choose(settings, app.as_ref());
            // An outside format change retires an earlier preview even before
            // its event has reached the web view. Held choices remain explicit.
            let format = serde_json::to_string(&choice.output).unwrap_or_default();
            (format!("default:{}:{format}", target.owner()), None, None)
        }
        Err(_) => (format!("unavailable:project:{}", target.owner()), None, None),
    };
    crate::project::activate_context(app, key, path, output);
    active
}

/// The result is the replacement runtime-only choice. Definitions and saved
/// app bindings are changed on the transaction's private copy, never early.
fn edit(
    settings: &mut Settings,
    target: &Target,
    session: Option<&str>,
    action: &WritingEdit,
) -> Result<Option<String>, ChangeError> {
    if target.editor {
        return Err(ChangeError::Editor);
    }
    let mut session = session.map(str::to_owned);
    match action {
        WritingEdit::Create { name, output } => {
            let project = definitions::create(settings, name.clone(), output.spec().clone(), None)?;
            select(settings, target, &mut session, Some(project.name));
        }
        WritingEdit::Select { id } => {
            if let Some(id) = id {
                let project = definitions::find(settings, id)?;
                crate::output::of_writing(&project.format).map_err(|_| ChangeError::Unsupported)?;
            }
            select(settings, target, &mut session, id.clone());
        }
        WritingEdit::Rename { id, name } => {
            definitions::rename(settings, id, name.clone())?;
            if session.as_deref() == Some(id) {
                session = Some(name.clone());
            }
        }
        WritingEdit::Remove { id } => {
            definitions::remove(settings, id)?;
            if session.as_deref() == Some(id) {
                session = None;
            }
        }
        WritingEdit::Output { id, output } => {
            let bibliography = definitions::find(settings, id)?.bibliography.clone();
            definitions::update(settings, id, output.spec().clone(), bibliography)?;
        }
    }
    Ok(session)
}

fn select(settings: &mut Settings, target: &Target, session: &mut Option<String>, id: Option<String>) {
    if let Some(key) = &target.binding {
        if let Some(id) = id {
            settings.device.writing_apps.insert(key.clone(), id);
        } else {
            settings.device.writing_apps.remove(key);
        }
    } else {
        *session = id;
    }
}

/// Output, edit authority and preview authority from one immutable native observation.
pub(crate) struct DestinationContext {
    pub(crate) settings: Settings,
    pub(crate) active: Option<ActiveWriting>,
    pub(crate) edit: String,
    pub(crate) version: (u32, u32),
}

pub(crate) fn destination_context(
    app: &AppHandle,
    captured: Option<&AppId>,
) -> Result<DestinationContext, CommandError> {
    let target = target(app, captured);
    if target.editor {
        let version = app.state::<crate::project::ProjectBibliography>().version();
        return Ok(DestinationContext {
            settings: Settings::default(),
            active: None,
            edit: context_token_for(&target, version.0),
            version,
        });
    }
    let runtime = app.state::<Runtime>();
    let session = runtime.0.lock().map_err(|_| CommandError::from(ChangeError::Stale))?;
    if current_target(app) != target {
        return Err(ChangeError::Destination.into());
    }
    let settings = saved_settings(app, &target)?;
    let active = activate(app, &settings, &target, session.as_deref())?;
    let version = app.state::<crate::project::ProjectBibliography>().version();
    if current_target(app) != target {
        return Err(ChangeError::Destination.into());
    }
    Ok(DestinationContext {
        settings,
        active,
        edit: context_token_for(&target, version.0),
        version,
    })
}

pub(crate) fn refresh(app: &AppHandle, captured: Option<&AppId>) -> Result<Option<ActiveWriting>, CommandError> {
    destination_context(app, captured).map(|context| context.active)
}

fn shown(app: &AppHandle, settings: &Settings, target: &Target, session: Option<&str>) -> WritingsView {
    // List an unsupported selected project so its definition can be repaired.
    let _ = activate(app, settings, target, session);
    let mut view = view_for(settings, target, session);
    view.context = context_token(app, target);
    view
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn writings(app: AppHandle) -> Result<WritingsView, CommandError> {
    let target = current_target(&app);
    let runtime = app.state::<Runtime>();
    let session = runtime.0.lock().map_err(|_| CommandError::from(ChangeError::Stale))?;
    if current_target(&app) != target {
        return Err(CommandError::from(ChangeError::Destination));
    }
    if target.editor {
        return Err(CommandError::from(ChangeError::Editor));
    }
    let settings = saved_settings(&app, &target)?;
    Ok(shown(&app, &settings, &target, session.as_deref()))
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn edit_writing(app: AppHandle, context: String, action: WritingEdit) -> Result<WritingsView, CommandError> {
    let target = current_target(&app);
    if context_token(&app, &target) != context {
        return Err(CommandError::from(ChangeError::Destination));
    }
    if target.editor {
        return Err(CommandError::from(ChangeError::Editor));
    }
    let runtime = app.state::<Runtime>();
    let mut session = runtime.0.lock().map_err(|_| CommandError::from(ChangeError::Stale))?;
    let (settings, replacement) = app
        .state::<SettingsFile>()
        .owner
        .change(|settings| {
            if current_target(&app) != target || context_token(&app, &target) != context {
                return Err(ChangeError::Destination);
            }
            edit(settings, &target, session.as_deref(), &action)
        })
        .map_err(CommandError::from)?;
    // Only a save changes the session-only choice.
    if target.binding.is_none() {
        *session = replacement;
    }
    let current = current_target(&app);
    if current != target {
        let _ = activate(&app, &settings, &current, session.as_deref());
        let _ = app.emit("writings:changed", ());
        return Err(CommandError::from(ChangeError::Destination));
    }
    let view = shown(&app, &settings, &target, session.as_deref());
    let _ = app.emit("writings:changed", ());
    Ok(view)
}

/// Immutable owner captured before opening a bibliography chooser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BibliographyOwner {
    target: Target,
    id: Option<String>,
    definition: Option<Writing>,
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
    let id = selected(settings, target, session);
    Ok(BibliographyOwner {
        target: target.clone(),
        id: id.map(str::to_owned),
        definition: id.and_then(|id| definitions::find(settings, id).ok()).cloned(),
        activation,
    })
}

pub(crate) fn bibliography_owner(app: &AppHandle) -> Result<BibliographyOwner, CommandError> {
    let target = current_target(app);
    let runtime = app.state::<Runtime>();
    let session = runtime.0.lock().map_err(|_| CommandError::from(ChangeError::Stale))?;
    if current_target(app) != target {
        return Err(ChangeError::Destination.into());
    }
    let settings = read_settings(&app.state::<SettingsFile>().path)?;
    let activation = app.state::<crate::project::ProjectBibliography>().version().0;
    owner_for(&settings, &target, session.as_deref(), activation).map_err(CommandError::from)
}

pub(crate) fn set_bibliography(
    app: &AppHandle,
    owner: &BibliographyOwner,
    path: Option<PathBuf>,
) -> Result<(), CommandError> {
    let target = current_target(app);
    if target != owner.target {
        return Err(ChangeError::Destination.into());
    }
    let runtime = app.state::<Runtime>();
    let session = runtime.0.lock().map_err(|_| CommandError::from(ChangeError::Stale))?;
    let (settings, ()) = app
        .state::<SettingsFile>()
        .owner
        .change(|settings| {
            let activation = app.state::<crate::project::ProjectBibliography>().version().0;
            if current_target(app) != target || owner_for(settings, &target, session.as_deref(), activation)? != *owner
            {
                return Err(ChangeError::Stale);
            }
            if let Some(id) = &owner.id {
                let output = definitions::find(settings, id)?.format.clone();
                definitions::update(settings, id, output, path)?;
            } else {
                return Err(ChangeError::Stale);
            }
            Ok::<_, ChangeError>(())
        })
        .map_err(CommandError::from)?;
    let current = current_target(app);
    let activation = activate(app, &settings, &current, session.as_deref());
    let _ = app.emit("writings:changed", ());
    if current != target {
        return Err(ChangeError::Destination.into());
    }
    activation.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> OutputRef {
        crate::output::stored(Some("pandoc"))
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
            &WritingEdit::Create {
                name: name.into(),
                output: output(),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        settings.device.writing_apps[target.binding.as_ref().unwrap_or_else(|| panic!("an app"))].clone()
    }

    #[test]
    fn switching_projects_changes_output_and_bibliography_together_and_leaving_restores_the_app() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let first = create(&mut settings, &target, "Thesis");
        definitions::update(
            &mut settings,
            &first,
            output().spec().clone(),
            Some(invented("thesis.bib")),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let second = create(&mut settings, &target, "Notes");
        edit(
            &mut settings,
            &target,
            None,
            &WritingEdit::Output {
                id: second.clone(),
                output: crate::output::stored(Some("apa")),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(selected(&settings, &target, None), Some(second.as_str()));
        assert_eq!(
            active_for(&settings, &target, None)
                .unwrap_or_else(|error| panic!("{error}"))
                .unwrap_or_else(|| panic!("an active project"))
                .output,
            crate::output::stored(Some("apa"))
        );
        edit(
            &mut settings,
            &target,
            None,
            &WritingEdit::Select {
                id: Some(first.clone()),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let active = active_for(&settings, &target, None)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("an active project"));
        assert_eq!(active.output, output());
        assert_eq!(active.bibliography, Some(invented("thesis.bib")));
        edit(&mut settings, &target, None, &WritingEdit::Select { id: None }).unwrap_or_else(|error| panic!("{error}"));
        assert!(
            active_for(&settings, &target, None)
                .unwrap_or_else(|error| panic!("{error}"))
                .is_none()
        );
        assert_eq!(settings.device.writings.len(), 2);
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
            &WritingEdit::Select {
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
                &WritingEdit::Select { id: Some(project) }
            )
            .is_err()
        );
        assert_eq!(settings.device.writing_apps.len(), 1);
    }

    #[test]
    fn failed_persistence_leaves_the_selection_and_definition_as_they_were() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let target = known("example.editor", 1);
        let (settings, _) = ibis_engine::settings::Owner::open(path.clone())
            .change(|settings| {
                edit(
                    settings,
                    &target,
                    None,
                    &WritingEdit::Create {
                        name: "Paper".into(),
                        output: output(),
                    },
                )
            })
            .unwrap_or_else(|error| panic!("{error}"));
        let id = selected(&settings, &target, None)
            .unwrap_or_else(|| panic!("a selected project"))
            .to_owned();
        std::fs::remove_file(&path).unwrap_or_else(|error| panic!("{error}"));
        std::fs::create_dir(&path).unwrap_or_else(|error| panic!("{error}"));
        let changed = ibis_engine::settings::Owner::open(path.clone()).change(|settings| {
            edit(
                settings,
                &target,
                None,
                &WritingEdit::Rename {
                    id,
                    name: "Unsaved".into(),
                },
            )
        });
        assert!(changed.is_err());
        assert_eq!(settings.device.writings[0].name, "Paper");
    }

    #[test]
    fn project_views_never_reveal_the_bibliography_folder() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let id = create(&mut settings, &target, "Paper");
        definitions::update(
            &mut settings,
            &id,
            output().spec().clone(),
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
        let (settings, ()) = ibis_engine::settings::Owner::open(path.clone())
            .change(|settings| {
                let id = create(settings, &target, "Thesis");
                definitions::update(settings, &id, output().spec().clone(), Some(invented("thesis.bib")))?;
                Ok::<_, ChangeError>(())
            })
            .unwrap_or_else(|error| panic!("{error}"));
        let reopened = crate::settings_fixture::read(&path);
        let before = active_for(&settings, &target, None).unwrap_or_else(|error| panic!("{error}"));
        let after = active_for(&reopened, &known("example.editor", 2), None).unwrap_or_else(|error| panic!("{error}"));
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
        edit(&mut settings, &target, None, &WritingEdit::Select { id: Some(first) })
            .unwrap_or_else(|error| panic!("{error}"));
        let current = owner_for(&settings, &target, None, 3).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(earlier.id, current.id);
        assert_ne!(earlier, current, "a matching definition never revives an old chooser");
    }

    #[test]
    fn a_delayed_bibliography_save_refuses_a_newer_definition_of_the_same_writing() {
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let name = create(&mut settings, &target, "Thesis");
        let earlier = owner_for(&settings, &target, None, 1).unwrap_or_else(|error| panic!("{error}"));
        definitions::update(
            &mut settings,
            &name,
            crate::output::stored(Some("apa")).spec().clone(),
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let current = owner_for(&settings, &target, None, 1).unwrap_or_else(|error| panic!("{error}"));
        assert_ne!(earlier, current, "a file edit must retire an already open chooser");
    }

    #[test]
    fn unsupported_output_is_visible_in_the_manager_and_refused_before_selecting() {
        use ibis_core::contract::citation::OutputSpec;
        let mut settings = Settings::default();
        let target = known("example.editor", 1);
        let existing = create(&mut settings, &target, "Thesis");
        let other = definitions::create(
            &mut settings,
            "Other workflow".into(),
            OutputSpec::Csl {
                style: "elsevier-harvard".into(),
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
                &WritingEdit::Select {
                    id: Some(other.name.clone())
                }
            )
            .is_err()
        );
        assert_eq!(settings, before);
        let shown = view_for(&settings, &target, None);
        assert_eq!(shown.selected.as_deref(), Some(existing.as_str()));
        assert!(
            shown
                .writings
                .iter()
                .any(|project| project.name == other.name && project.output.is_none())
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
                &WritingEdit::Create {
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
            &WritingEdit::Select {
                id: Some(project.clone()),
            },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        edit(
            &mut settings,
            &target,
            Some(&project),
            &WritingEdit::Remove { id: project.clone() },
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(settings.device.writings.is_empty());
        assert!(settings.device.writing_apps.is_empty());
        assert_eq!(
            settings.portable.paste, before.portable.paste,
            "project selection never grants paste permission"
        );
    }

    #[test]
    fn every_picker_output_round_trips_through_the_shared_project_contract() {
        for listed in ibis_engine::outputs::outputs() {
            let output = crate::output::stored(Some(&listed.id));
            assert_eq!(crate::output::of_writing(&output.spec().clone()), Ok(output));
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
        ] {
            assert!(crate::output::of_writing(&output).is_err());
        }
    }

    #[test]
    fn unreadable_project_settings_cannot_silently_choose_default_output() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        std::fs::write(&path, "not settings").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            read_settings(&path),
            Err(CommandError::Presented(
                (&settings::SettingsError::Invalid(String::new())).into()
            ))
        );
        std::fs::remove_file(&path).unwrap_or_else(|error| panic!("{error}"));
        std::fs::create_dir(&path).unwrap_or_else(|error| panic!("{error}"));
        assert!(read_settings(&path).is_err());
    }

    #[test]
    fn an_unreadable_edit_never_substitutes_the_backups_older_writing() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let target = known("example.editor", 1);
        let (settings, _) = ibis_engine::settings::Owner::open(path.clone())
            .change(|settings| {
                edit(
                    settings,
                    &target,
                    None,
                    &WritingEdit::Create {
                        name: "Paper".into(),
                        output: output(),
                    },
                )
            })
            .unwrap_or_else(|error| panic!("{error}"));
        std::fs::copy(&path, path.with_extension("json.bak")).unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(&path, "not settings").unwrap_or_else(|error| panic!("{error}"));
        assert!(read_settings(&path).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{error}")),
            "not settings"
        );
        assert!(!settings.device.writings.is_empty());
    }
}
