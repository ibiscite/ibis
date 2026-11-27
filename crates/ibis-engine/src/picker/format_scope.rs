//! A native picker can remember a format only for its captured destination.
use crate::format::{self, FormatFrom};
use crate::settings::{Owner, Settings, SettingsError};
use ibis_core::contract::{EngineError, ErrorCode};
use ibis_core::destination::AppId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerFormatScope {
    pub format: Option<String>,
    pub from: FormatFrom,
    pub usual_format: Option<String>,
    pub remembers_for: Option<String>,
    pub remembered_for: Option<String>,
    pub project: Option<String>,
    /// This native picker cannot establish the project's bibliography proof.
    pub requires_bibliography: bool,
}

impl PickerFormatScope {
    pub fn is_current(&self, saved: Option<&Settings>, app: Option<&AppId>, site: Option<&AppId>) -> bool {
        if self.requires_bibliography {
            return false;
        }
        let defaults = Settings::default();
        *self == Self::for_destination(saved.unwrap_or(&defaults), app, site)
    }

    pub(super) fn for_destination(saved: &Settings, app: Option<&AppId>, site: Option<&AppId>) -> Self {
        let choice = format::choose_at(saved, app, site);
        let project = choice.writing.map(|writing| writing.name);
        let requires_bibliography = project.as_ref().is_some_and(|name| {
            saved
                .device
                .writings
                .iter()
                .any(|writing| &writing.name == name && writing.bibliography.is_some())
        });
        Self {
            format: choice.id,
            from: choice.from,
            usual_format: format::usual_at(saved, app, site).id,
            remembers_for: format::remembers_for(app, site).map(|owner| owner.id),
            remembered_for: (choice.from == FormatFrom::App)
                .then(|| format::remembered_for(saved, app, site).map(|owner| owner.id.clone()))
                .flatten(),
            project,
            requires_bibliography,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerChangeFormatRequest {
    pub expected: Option<PickerFormatScope>,
    pub remember: Option<String>,
    pub forget: Option<bool>,
}

impl PickerChangeFormatRequest {
    pub fn answer(
        self,
        owner: &Owner,
        app: Option<&AppId>,
        site: Option<&AppId>,
    ) -> Result<PickerFormatScope, EngineError> {
        let scope = |saved: &Settings| PickerFormatScope::for_destination(saved, app, site);
        match (self.expected, self.remember, self.forget) {
            (None, None, None) => Ok(scope(
                owner.read().cites().map_err(crate::settings_backup::settings_code)?,
            )),
            (Some(expected), remember, forget) if remember.is_some() ^ (forget == Some(true)) => {
                let target = format::remembers_for(app, site).ok_or(ErrorCode::BadInput)?;
                if let Some(id) = remember.as_deref() {
                    crate::outputs::find(id)
                        .filter(|entry| !entry.exports_records && !entry.zotero_items)
                        .ok_or(ErrorCode::BadInput)?;
                }
                let (_, next) = owner
                    .change(|saved| {
                        let current = scope(saved);
                        if current != expected || current.project.is_some() {
                            return Ok::<_, SettingsError>(None);
                        }
                        if let Some(id) = remember {
                            saved.remember_app_output(&target, id);
                        } else if current.remembered_for.is_some() {
                            format::forget_at(saved, app, site);
                        } else {
                            return Ok(None);
                        }
                        Ok(Some(scope(saved)))
                    })
                    .map_err(|error| crate::settings_backup::settings_code(&error))?;
                next.ok_or_else(|| ErrorCode::BadInput.into())
            }
            _ => Err(ErrorCode::BadInput.into()),
        }
    }
}
