//! Optional named writing contexts, directly editable in settings.json.
use super::{Settings, SettingsError};
use ibis_core::contract::OutputSpec;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

pub const MAX_WRITINGS: usize = 64;
pub const MAX_NAME: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Writing {
    pub name: String,
    #[serde(with = "format")]
    pub format: OutputSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bibliography: Option<PathBuf>,
}

mod format {
    use super::*;
    use serde::{Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &OutputSpec, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(output) = crate::outputs::named(value) {
            output.id.serialize(serializer)
        } else {
            value.serialize(serializer)
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<OutputSpec, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value {
            Name(String),
            Spec(OutputSpec),
        }
        match Value::deserialize(deserializer)? {
            Value::Spec(spec) => Ok(spec),
            Value::Name(name) => crate::outputs::find(&name)
                .map(|output| output.output.clone())
                .or_else(|| {
                    ibis_csl::style_forms(&name)
                        .ok()
                        .map(|_| OutputSpec::Csl { style: name })
                })
                .ok_or_else(|| serde::de::Error::custom("unknown writing format")),
        }
    }
}
#[derive(Debug, Error)]
pub enum WritingError {
    #[error("Give the writing a name of 1 to 80 characters, without control characters.")]
    InvalidName,
    #[error("A writing already has that name. Choose another name.")]
    DuplicateName,
    #[error("That writing is no longer available. Choose a writing again.")]
    NotFound,
    #[error("Ibis can remember at most 64 writings. Remove one before adding another.")]
    TooMany,
    #[error("The writing definitions are not valid.")]
    Invalid,
    #[error("The document context could not be read.")]
    ContextUnavailable,
    #[error("The document context must name one bibliography and CSL style as plain values.")]
    InvalidContext,
    #[error("More than one writing matches this context. Choose a writing by name.")]
    AmbiguousContext,
    #[error("The writing could not be saved. Try again.")]
    Settings(#[from] SettingsError),
}
fn name_key(name: &str) -> String {
    name.nfkc().flat_map(char::to_lowercase).collect()
}
pub fn validate(writings: &[Writing]) -> Result<(), WritingError> {
    if writings.len() > MAX_WRITINGS {
        return Err(WritingError::TooMany);
    }
    let mut names = std::collections::BTreeSet::new();
    for writing in writings {
        if writing.name.is_empty()
            || writing.name.trim() != writing.name
            || writing.name.chars().count() > MAX_NAME
            || writing
                .name
                .chars()
                .any(|c| c.is_control() || ibis_core::model::is_hidden(c))
        {
            return Err(WritingError::InvalidName);
        }
        if !names.insert(name_key(&writing.name)) {
            return Err(WritingError::DuplicateName);
        }
        if writing.bibliography.as_ref().is_some_and(|path| {
            !path.is_absolute() || !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("bib"))
        }) {
            return Err(WritingError::Invalid);
        }
        let format = crate::outputs::resolve(&writing.format).map_err(|_| WritingError::Invalid)?;
        match &format {
            OutputSpec::Csl { style } | OutputSpec::Preset { name: style }
                if style.is_empty() || style.len() > 256 || style.chars().any(char::is_control) =>
            {
                return Err(WritingError::Invalid);
            }
            OutputSpec::Key {
                command: Some(command), ..
            } if command.is_empty() || command.len() > 128 || command.chars().any(char::is_control) => {
                return Err(WritingError::Invalid);
            }
            OutputSpec::Csl { style } => {
                ibis_csl::style_forms(style).map_err(|_| WritingError::Invalid)?;
            }
            OutputSpec::Key { .. } => {
                ibis_core::syntax::capabilities(&format).map_err(|_| WritingError::Invalid)?;
            }
            _ => {}
        }
    }
    Ok(())
}
pub fn find<'a>(settings: &'a Settings, name: &str) -> Result<&'a Writing, WritingError> {
    settings
        .writings
        .iter()
        .find(|writing| writing.name == name)
        .ok_or(WritingError::NotFound)
}
pub fn create(
    settings: &mut Settings,
    name: String,
    format: OutputSpec,
    bibliography: Option<PathBuf>,
) -> Result<Writing, WritingError> {
    let writing = Writing {
        name,
        format,
        bibliography,
    };
    let mut proposed = settings.writings.clone();
    proposed.push(writing.clone());
    validate(&proposed)?;
    settings.writings = proposed;
    Ok(writing)
}
pub fn rename(settings: &mut Settings, name: &str, new_name: String) -> Result<Writing, WritingError> {
    let mut proposed = settings.writings.clone();
    let writing = proposed
        .iter_mut()
        .find(|writing| writing.name == name)
        .ok_or(WritingError::NotFound)?;
    writing.name = new_name.clone();
    let renamed = writing.clone();
    validate(&proposed)?;
    settings.writings = proposed;
    for selected in settings
        .writing_apps
        .values_mut()
        .filter(|selected| selected.as_str() == name)
    {
        *selected = new_name.clone();
    }
    Ok(renamed)
}
pub fn update(
    settings: &mut Settings,
    name: &str,
    format: OutputSpec,
    bibliography: Option<PathBuf>,
) -> Result<Writing, WritingError> {
    let mut proposed = settings.writings.clone();
    let writing = proposed
        .iter_mut()
        .find(|writing| writing.name == name)
        .ok_or(WritingError::NotFound)?;
    writing.format = format;
    writing.bibliography = bibliography;
    let updated = writing.clone();
    validate(&proposed)?;
    settings.writings = proposed;
    Ok(updated)
}
pub fn remove(settings: &mut Settings, name: &str) -> Result<Writing, WritingError> {
    let writing = find(settings, name)?.clone();
    settings.writings.retain(|writing| writing.name != name);
    settings.writing_apps.retain(|_, selected| selected != name);
    Ok(writing)
}
/// A whole checked definition edit, serialized through the settings owner.
pub fn change<T: Send + 'static>(
    path: &Path,
    change: impl FnOnce(&mut Settings) -> Result<T, WritingError>,
) -> Result<T, WritingError> {
    super::Owner::open(path.to_path_buf())
        .change(change)
        .map(|(_, result)| result)
}

/// Select only from an editor/CLI's explicit context. A document name, sibling
/// bibliography, window title or URL never selects a writing. Only front matter
/// explicitly naming a bibliography or CSL style can do that.
pub fn from_context(
    settings: &Settings,
    document: Option<&Path>,
    bibliography: Option<&Path>,
    csl: Option<&str>,
) -> Result<Option<Writing>, WritingError> {
    let mut bibliography = bibliography.map(Path::to_path_buf);
    let mut csl = csl.map(str::to_owned);
    if let Some(document) = document {
        if document.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("bib")) {
            bibliography.get_or_insert_with(|| document.to_path_buf());
        } else if document.extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("qmd") || ext.eq_ignore_ascii_case("markdown")
        }) {
            let text = super::read_regular(document).map_err(|_| WritingError::ContextUnavailable)?;
            let mut lines = text.lines();
            if lines.next().is_some_and(|line| line.trim() == "---") {
                let mut closed = false;
                let mut seen = std::collections::BTreeSet::new();
                for line in lines {
                    if matches!(line.trim(), "---" | "...") {
                        closed = true;
                        break;
                    }
                    if line.starts_with(char::is_whitespace) {
                        continue;
                    }
                    let Some((key, value)) = line.split_once(':') else {
                        continue;
                    };
                    if !matches!(key, "bibliography" | "csl") {
                        continue;
                    }
                    if !seen.insert(key) {
                        return Err(WritingError::InvalidContext);
                    }
                    let value = scalar(value)?;
                    match key {
                        "bibliography" if bibliography.is_none() => {
                            let path = PathBuf::from(value);
                            bibliography = Some(if path.is_absolute() {
                                path
                            } else {
                                document.parent().unwrap_or(Path::new(".")).join(path)
                            });
                        }
                        "csl" if csl.is_none() => csl = Some(value),
                        _ => {}
                    }
                }
                if !closed {
                    return Err(WritingError::InvalidContext);
                }
            }
        }
    }
    if bibliography.is_none() && csl.is_none() {
        return Ok(None);
    }
    let matches: Vec<_> = settings
        .writings
        .iter()
        .filter(|writing| {
            bibliography.as_ref().is_none_or(|path| {
                writing
                    .bibliography
                    .as_ref()
                    .is_some_and(|saved| same_path(saved, path))
            }) && csl
                .as_ref()
                .is_none_or(|style| match crate::outputs::resolve(&writing.format) {
                    Ok(OutputSpec::Csl { style: saved }) => style_id(style) == saved,
                    _ => false,
                })
        })
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [writing] => Ok(Some((*writing).clone())),
        _ => Err(WritingError::AmbiguousContext),
    }
}
fn style_id(style: &str) -> &str {
    style.strip_suffix(".csl").unwrap_or(style)
}
fn same_path(a: &Path, b: &Path) -> bool {
    fn normalized(path: &Path) -> PathBuf {
        if let Ok(path) = std::fs::canonicalize(path) {
            return path;
        }
        let mut result = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    result.pop();
                }
                component => result.push(component.as_os_str()),
            }
        }
        result
    }
    normalized(a) == normalized(b)
}
fn scalar(value: &str) -> Result<String, WritingError> {
    let value = value.trim();
    if value.starts_with('"') {
        return serde_json::from_str(value).map_err(|_| WritingError::InvalidContext);
    }
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        return Ok(value[1..value.len() - 1].replace("''", "'"));
    }
    if value.is_empty() || value.starts_with(['[', '{', '&', '*', '|', '>']) || value.contains(" #") {
        return Err(WritingError::InvalidContext);
    }
    Ok(value.to_owned())
}

use ibis_core::contract::{Problem, Recovery, RecoveryInfo};

impl From<&crate::settings::writings::WritingError> for Problem {
    fn from(error: &crate::settings::writings::WritingError) -> Self {
        use crate::settings::writings::WritingError as P;
        let (code, recovery) = match error {
            P::InvalidName => ("invalid-name", Recovery::RenameProject),
            P::DuplicateName => ("duplicate-name", Recovery::RenameProject),
            P::NotFound => ("not-found", Recovery::ChooseProject),
            P::TooMany => ("too-many", Recovery::RemoveProject),
            P::Invalid => ("invalid", Recovery::ReviewSettings),
            P::ContextUnavailable => ("context-unavailable", Recovery::Retry),
            P::InvalidContext => ("invalid-context", Recovery::EditInput),
            P::AmbiguousContext => ("ambiguous-context", Recovery::ChooseProject),
            P::Settings(error) => return error.into(),
        };
        Self {
            code: format!("project-{code}"),
            info: RecoveryInfo::new(&format!("app-project-error-{code}"), recovery),
        }
    }
}
