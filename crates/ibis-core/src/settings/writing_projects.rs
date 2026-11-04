//! Explicit writing contexts shared by the app and command line. Selection is
//! separate from the definition; an app choice never grants delivery permission.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use super::{Settings, SettingsError};
use crate::contract::OutputSpec;

pub const MAX_PROJECTS: usize = 64;
pub const MAX_NAME: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WritingProject {
    pub id: String,
    /// Opaque edit identity, independent of names, paths and output text.
    revision: String,
    pub name: String,
    pub output: OutputSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bibliography: Option<PathBuf>,
}

impl WritingProject {
    /// Changes with every definition edit, including a change back to old data.
    /// This token contains no digest of a private bibliography path.
    pub fn revision(&self) -> String {
        self.revision.clone()
    }
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("Give the project a name of 1 to 80 characters, without control characters.")]
    InvalidName,
    #[error("A writing project already has that name. Choose another name.")]
    DuplicateName,
    #[error("That writing project is no longer available. Choose a project again.")]
    NotFound,
    #[error("Ibis can remember at most 64 writing projects. Remove one before adding another.")]
    TooMany,
    #[error("The writing-project definitions are not valid.")]
    Invalid,
    #[error("The writing project could not be saved. Try again.")]
    Settings(#[from] SettingsError),
}

fn name_key(name: &str) -> String {
    name.nfkc().flat_map(char::to_lowercase).collect()
}

fn check_name(name: &str) -> Result<(), ProjectError> {
    if name.is_empty()
        || name.trim() != name
        || name.chars().count() > MAX_NAME
        || name
            .chars()
            .any(|character| character.is_control() || crate::model::is_hidden(character))
    {
        Err(ProjectError::InvalidName)
    } else {
        Ok(())
    }
}

pub fn validate(projects: &[WritingProject]) -> Result<(), ProjectError> {
    if projects.len() > MAX_PROJECTS {
        return Err(ProjectError::TooMany);
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    for project in projects {
        check_name(&project.name)?;
        if !is_token(&project.id) || !is_token(&project.revision) || !ids.insert(&project.id) {
            return Err(ProjectError::Invalid);
        }
        if !names.insert(name_key(&project.name)) {
            return Err(ProjectError::DuplicateName);
        }
        if project
            .bibliography
            .as_ref()
            .is_some_and(|path| !path.is_absolute() || path.file_name().is_none())
        {
            return Err(ProjectError::Invalid);
        }
        match &project.output {
            OutputSpec::Csl { style } | OutputSpec::Preset { name: style }
                if style.is_empty() || style.len() > 256 || style.chars().any(char::is_control) =>
            {
                return Err(ProjectError::Invalid);
            }
            OutputSpec::Key {
                command: Some(command), ..
            } if command.is_empty() || command.len() > 128 || command.chars().any(char::is_control) => {
                return Err(ProjectError::Invalid);
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn find<'a>(settings: &'a Settings, id: &str) -> Result<&'a WritingProject, ProjectError> {
    settings
        .writing_projects
        .iter()
        .find(|project| project.id == id)
        .ok_or(ProjectError::NotFound)
}

fn new_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut digest = Sha256::new();
    digest.update(b"ibis-writing-project-v1");
    digest.update(std::process::id().to_le_bytes());
    digest.update(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_le_bytes(),
    );
    digest.update(NEXT.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    hex(&digest.finalize()[..16])
}

fn is_token(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                DIGITS[(byte >> 4) as usize] as char,
                DIGITS[(byte & 15) as usize] as char,
            ]
        })
        .collect()
}

pub fn create(
    settings: &mut Settings,
    name: String,
    output: OutputSpec,
    bibliography: Option<PathBuf>,
) -> Result<WritingProject, ProjectError> {
    let project = WritingProject {
        id: new_id(),
        revision: new_id(),
        name,
        output,
        bibliography,
    };
    let mut projects = settings.writing_projects.clone();
    projects.push(project.clone());
    validate(&projects)?;
    settings.writing_projects = projects;
    Ok(project)
}

pub fn rename(settings: &mut Settings, id: &str, name: String) -> Result<WritingProject, ProjectError> {
    let mut projects = settings.writing_projects.clone();
    let project = projects
        .iter_mut()
        .find(|project| project.id == id)
        .ok_or(ProjectError::NotFound)?;
    if project.name != name {
        project.name = name;
        project.revision = new_id();
    }
    let renamed = project.clone();
    validate(&projects)?;
    settings.writing_projects = projects;
    Ok(renamed)
}

pub fn update(
    settings: &mut Settings,
    id: &str,
    output: OutputSpec,
    bibliography: Option<PathBuf>,
) -> Result<WritingProject, ProjectError> {
    let mut projects = settings.writing_projects.clone();
    let project = projects
        .iter_mut()
        .find(|project| project.id == id)
        .ok_or(ProjectError::NotFound)?;
    if project.output != output || project.bibliography != bibliography {
        project.output = output;
        project.bibliography = bibliography;
        project.revision = new_id();
    }
    let updated = project.clone();
    validate(&projects)?;
    settings.writing_projects = projects;
    Ok(updated)
}

pub fn remove(settings: &mut Settings, id: &str) -> Result<WritingProject, ProjectError> {
    let project = find(settings, id)?.clone();
    settings.writing_projects.retain(|project| project.id != id);
    settings.writing_project_apps.retain(|_, selected| selected != id);
    Ok(project)
}

/// A whole definition edit, never an optimistic in-memory save. A refusal or
/// failed write changes neither the stored definition nor the caller's context.
pub fn change<T>(
    path: &Path,
    change: impl FnOnce(&mut Settings) -> Result<T, ProjectError>,
) -> Result<T, ProjectError> {
    super::transaction(path, change).map(|(_, result)| result)
}
