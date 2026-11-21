//! Per-app writing, bibliography and format ownership (decisions 0052 and 0054).
pub mod output;
pub mod project;
use crate::settings::writings::{self as definitions, Writing, WritingError};
use crate::settings::{self, Settings};
use ibis_core::destination::{AppId, System};
use output::OutputRef;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
const CHANGED: &str = "The project changed. Open the format menu again.";
const REOPEN: &str = "The destination changed. Open the format menu again.";
const EDITOR: &str = "This editor request chooses its own output. Finish it before changing projects.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct WritingView {
    pub id: String,
    pub name: String,
    /// A writing can name outputs not yet available in the picker.
    #[schemars(with = "Option<String>")]
    pub output: Option<OutputRef>,
    pub bibliography: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct WritingsView {
    pub writings: Vec<WritingView>,
    pub selected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<WritingView>,
    pub session_only: bool,
    /// Owns one picker opening and captured destination; never a path or title.
    pub context: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum WritingEdit {
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
pub enum Error {
    Project(WritingError),
    Stale,
    Destination,
    Editor,
    Unsupported,
    SnapshotChanged,
    Cancelled,
    Session(crate::citations::SessionError),
    Presented(ibis_core::contract::Problem),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => error.fmt(formatter),
            Self::Stale => formatter.write_str(CHANGED),
            Self::Destination => formatter.write_str(REOPEN),
            Self::Editor => formatter.write_str(EDITOR),
            Self::SnapshotChanged => formatter.write_str("The context changed."),
            Self::Cancelled => formatter.write_str("Cancelled."),
            Self::Session(error) => error.fmt(formatter),
            Self::Presented(problem) => {
                formatter.write_str(&crate::catalogue::problem(problem, crate::catalogue::Audience::App))
            }
            Self::Unsupported => {
                formatter.write_str("This project's format is not available in the picker. Choose another format.")
            }
        }
    }
}
impl From<WritingError> for Error {
    fn from(error: WritingError) -> Self {
        Self::Project(error)
    }
}
impl From<settings::SettingsError> for Error {
    fn from(error: settings::SettingsError) -> Self {
        Self::Project(WritingError::Settings(error))
    }
}

impl From<Error> for ibis_core::contract::Problem {
    fn from(error: Error) -> Self {
        use ibis_core::contract::{Problem, Recovery as R, RecoveryInfo};
        let (code, id, recovery) = match error {
            Error::Session(error) => return error.problem(),
            Error::Presented(problem) => return problem,
            Error::SnapshotChanged => ("snapshot_changed", "app-command-snapshot-changed", R::SelectReferences),
            Error::Cancelled => {
                return ibis_core::contract::EngineError::new(ibis_core::contract::ErrorCode::Cancelled).into();
            }
            Error::Project(error) => return (&error).into(),
            Error::Stale | Error::Destination => ("project-changed", "app-project-error-changed", R::ReopenProjectMenu),
            Error::Editor => ("editor-pending", "app-project-error-editor", R::FinishEditorRequest),
            Error::Unsupported => ("project-output-unavailable", "app-project-error-output", R::ChooseStyle),
        };
        Problem {
            code: code.into(),
            info: RecoveryInfo::new(id, recovery),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    binding: Option<String>,
    pub generation: u64,
    pub editor: bool,
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

pub fn app_key(id: &AppId) -> String {
    let system = match id.system {
        System::Macos => "macos",
        System::Windows => "windows",
        System::Linux => "linux",
    };
    format!("{system}:{}", id.normalized())
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

pub fn saved_view(project: &Writing) -> WritingView {
    WritingView {
        id: project.name.clone(),
        name: project.name.clone(),
        output: output::of_writing(&project.format).ok(),
        bibliography: project.bibliography.as_deref().map(crate::project_name),
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

/// Internal context. The bibliography path never enters an IPC view.
#[derive(Debug, Clone)]
struct ActiveWriting {
    id: String,
    name: String,
    output: OutputRef,
    bibliography: Option<PathBuf>,
    context_key: String,
}

impl ActiveWriting {
    fn view(&self) -> WritingView {
        WritingView {
            id: self.id.clone(),
            name: self.name.clone(),
            output: Some(self.output),
            bibliography: self.bibliography.as_deref().map(crate::project_name),
        }
    }
}

fn active_for(settings: &Settings, target: &Target, session: Option<&str>) -> Result<Option<ActiveWriting>, Error> {
    let Some(id) = selected(settings, target, session) else {
        return Ok(None);
    };
    let project = definitions::find(settings, id).map_err(Error::from)?;
    Ok(Some(ActiveWriting {
        id: project.name.clone(),
        name: project.name.clone(),
        output: output::of_writing(&project.format).map_err(|_| Error::Unsupported)?,
        bibliography: project.bibliography.clone(),
        // The bibliography and output are compared on their own.
        context_key: format!("project:{}:{}", target.owner(), project.name),
    }))
}

/// Make the saved choice this destination's checking context.
fn activate(
    project: &project::ProjectBibliography,
    settings: &Settings,
    target: &Target,
    session: Option<&str>,
    app: Option<&AppId>,
) -> Result<Option<ActiveWriting>, Error> {
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
            let choice = crate::format::choose(settings, app);
            // An outside format change retires an earlier preview even before
            // its event has reached the web view. Held choices remain explicit.
            let format = serde_json::to_string(&choice.output).unwrap_or_default();
            (format!("default:{}:{format}", target.owner()), None, None)
        }
        Err(_) => (format!("unavailable:project:{}", target.owner()), None, None),
    };
    project.activate(key, path, output);
    active
}

/// The result is the replacement runtime-only choice. Definitions and saved
/// app bindings are changed on the transaction's private copy, never early.
fn edit(
    settings: &mut Settings,
    target: &Target,
    session: Option<&str>,
    action: &WritingEdit,
) -> Result<Option<String>, Error> {
    if target.editor {
        return Err(Error::Editor);
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
                output::of_writing(&project.format).map_err(|_| Error::Unsupported)?;
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

/// Immutable owner captured before opening a bibliography chooser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BibliographyOwner {
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
) -> Result<BibliographyOwner, Error> {
    if target.editor {
        return Err(Error::Editor);
    }
    let id = selected(settings, target, session);
    Ok(BibliographyOwner {
        target: target.clone(),
        id: id.map(str::to_owned),
        definition: id.and_then(|id| definitions::find(settings, id).ok()).cloned(),
        activation,
    })
}

/// Native capture supplied by the desktop. It carries no paste authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub app: Option<AppId>,
    pub generation: u64,
    pub editor: Option<EditorContext>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorContext {
    pub output: ibis_core::contract::OutputSpec,
    pub writing: Option<Writing>,
}
impl Capture {
    fn target(&self) -> Target {
        Target {
            binding: self.editor.is_none().then(|| self.app.as_ref().map(app_key)).flatten(),
            generation: self.generation,
            editor: self.editor.is_some(),
        }
    }
}

/// The one context boundary. Native dialogs and delivery stay with the host.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ContextRequest {
    Destination {},
    Writings {},
    EditWriting { context: String, action: WritingEdit },
    ProjectStatus {},
    RetryProject {},
    StopProject {},
}

/// A whole observation: output, writing, bibliography and edit authority agree.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ContextView {
    pub format: crate::format::FormatChoice,
    pub usual_format: String,
    pub writing: Option<WritingView>,
    pub context_version: (u32, u32),
    pub writing_context: Option<String>,
    pub writings: WritingsView,
    pub project: Option<project::ProjectStatus>,
    /// The format palette offers Projects: once one exists, or once this app
    /// has a second format saved, its own beside your usual one.
    pub offers_projects: bool,
    #[serde(skip)]
    pub paste_enabled: bool,
}

pub struct Owner {
    settings: Arc<settings::Owner>,
    session: Mutex<Option<String>>,
    project: project::ProjectBibliography,
}
impl Owner {
    pub fn new(settings: Arc<settings::Owner>, project: project::ProjectBibliography) -> Self {
        Self {
            settings,
            session: Mutex::new(None),
            project,
        }
    }
    fn token(&self, target: &Target) -> String {
        context_token_for(target, self.project.version().0)
    }
    fn settings(&self, target: &Target) -> Result<Settings, Error> {
        self.settings
            .read()
            .cites()
            .cloned()
            .map_err(|error| Error::Presented(error.into()))
            .inspect_err(|_| {
                self.project
                    .activate(format!("unavailable:settings:{}", target.owner()), None, None);
            })
    }
    pub fn request(
        &self,
        capture: &Capture,
        request: ContextRequest,
        current: impl Fn() -> bool,
    ) -> Result<ContextView, Error> {
        let target = capture.target();
        let mut session = self.session.lock().map_err(|_| Error::Stale)?;
        if !current() {
            return Err(Error::Destination);
        }
        // An editor's request is self-contained, including when settings cannot be read.
        let settings = if let Some(editor) = &capture.editor {
            if !matches!(
                request,
                ContextRequest::Destination {} | ContextRequest::ProjectStatus {}
            ) {
                return Err(Error::Editor);
            }
            let version = self.project.version();
            return Ok(ContextView {
                format: match &editor.writing {
                    Some(writing) => crate::format::for_writing(
                        &editor.output,
                        crate::format::WritingName {
                            id: writing.name.clone(),
                            name: writing.name.clone(),
                        },
                        None,
                    ),
                    None => crate::format::choose_with(&Settings::default(), None, Some(&editor.output)),
                },
                usual_format: crate::format::usual(&Settings::default(), None).id.unwrap_or_default(),
                writing: None,
                context_version: version,
                writing_context: None,
                writings: view_for(&Settings::default(), &target, None),
                project: None,
                offers_projects: false,
                paste_enabled: false,
            });
        } else {
            match &request {
                ContextRequest::EditWriting { context, action } => {
                    let (settings, replacement) = self.settings.change(|settings| {
                        if !current() {
                            return Err(Error::Destination);
                        }
                        // Observe outside edits before accepting the menu's token.
                        let _ = activate(
                            &self.project,
                            settings,
                            &target,
                            session.as_deref(),
                            capture.app.as_ref(),
                        );
                        if !current() || self.token(&target) != *context {
                            return Err(Error::Destination);
                        }
                        edit(settings, &target, session.as_deref(), action)
                    })?;
                    if target.binding.is_none() {
                        *session = replacement;
                    }
                    settings
                }
                ContextRequest::StopProject {} => {
                    let owner = owner_for(
                        &self.settings(&target)?,
                        &target,
                        session.as_deref(),
                        self.project.version().0,
                    )?;
                    self.save_bibliography(&target, session.as_deref(), &owner, None, &current)?
                }
                _ => self.settings(&target)?,
            }
        };
        let active = activate(
            &self.project,
            &settings,
            &target,
            session.as_deref(),
            capture.app.as_ref(),
        );
        // Unsupported definitions remain editable in the writing manager.
        let active = if matches!(
            request,
            ContextRequest::Writings {} | ContextRequest::EditWriting { .. }
        ) {
            active.ok().flatten()
        } else {
            active?
        };
        if matches!(request, ContextRequest::RetryProject {}) {
            self.project.recheck();
        }
        if !current() {
            return Err(Error::Destination);
        }
        let format = match &active {
            Some(writing) => crate::format::for_writing(
                writing.output.spec(),
                crate::format::WritingName {
                    id: writing.id.clone(),
                    name: writing.name.clone(),
                },
                capture.app.as_ref(),
            ),
            None => crate::format::choose(&settings, capture.app.as_ref()),
        };
        let mut writings = view_for(&settings, &target, session.as_deref());
        writings.context = self.token(&target);
        let (context_version, project) = self.project.observation();
        Ok(ContextView {
            format,
            usual_format: crate::format::usual(&settings, capture.app.as_ref())
                .id
                .unwrap_or_default(),
            writing: active.map(|writing| writing.view()),
            context_version,
            writing_context: Some(writings.context.clone()),
            offers_projects: crate::format::offers_projects(&settings, capture.app.as_ref()),
            writings,
            project,
            paste_enabled: settings.portable.paste(),
        })
    }
    pub fn bibliography_owner(
        &self,
        capture: &Capture,
        current: impl Fn() -> bool,
    ) -> Result<BibliographyOwner, Error> {
        let target = capture.target();
        let session = self.session.lock().map_err(|_| Error::Stale)?;
        if !current() {
            return Err(Error::Destination);
        }
        owner_for(
            &self.settings(&target)?,
            &target,
            session.as_deref(),
            self.project.version().0,
        )
    }
    fn save_bibliography(
        &self,
        target: &Target,
        session: Option<&str>,
        owner: &BibliographyOwner,
        path: Option<PathBuf>,
        current: impl Fn() -> bool,
    ) -> Result<Settings, Error> {
        self.settings
            .change(|settings| {
                if !current()
                    || *target != owner.target
                    || owner_for(settings, target, session, self.project.version().0)? != *owner
                {
                    return Err(Error::Stale);
                }
                let id = owner.id.as_ref().ok_or(Error::Stale)?;
                let output = definitions::find(settings, id)?.format.clone();
                definitions::update(settings, id, output, path)?;
                Ok::<_, Error>(())
            })
            .map(|(settings, ())| settings)
    }
    pub fn set_bibliography(
        &self,
        capture: &Capture,
        owner: &BibliographyOwner,
        path: Option<PathBuf>,
        current: impl Fn() -> bool,
    ) -> Result<(), Error> {
        let target = capture.target();
        let session = self.session.lock().map_err(|_| Error::Stale)?;
        let settings = self.save_bibliography(&target, session.as_deref(), owner, path, &current)?;
        activate(
            &self.project,
            &settings,
            &target,
            session.as_deref(),
            capture.app.as_ref(),
        )?;
        if !current() {
            return Err(Error::Destination);
        }
        Ok(())
    }
}
fn context_token_for(target: &Target, activation: u32) -> String {
    serde_json::to_string(&(target.context(), activation)).unwrap_or_default()
}
impl From<crate::citations::SessionError> for Error {
    fn from(error: crate::citations::SessionError) -> Self {
        Self::Session(error)
    }
}
impl std::error::Error for Error {}

#[cfg(test)]
#[path = "app_context/request_tests.rs"]
mod request_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> OutputRef {
        output::stored(Some("pandoc"))
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
    fn failed_persistence_leaves_the_selection_and_definition_as_they_were() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let target = known("example.editor", 1);
        let (settings, _) = crate::settings::Owner::open(path.clone())
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
        let changed = crate::settings::Owner::open(path.clone()).change(|settings| {
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
    fn an_unreadable_edit_never_substitutes_the_backups_older_writing() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("settings.json");
        let target = known("example.editor", 1);
        let (settings, _) = crate::settings::Owner::open(path.clone())
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
        assert!(settings::load(&path).cites().is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{error}")),
            "not settings"
        );
        assert!(!settings.device.writings.is_empty());
    }
}
