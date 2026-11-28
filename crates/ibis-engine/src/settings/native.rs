use super::{
    Appearance, DEFAULTS, Density, LibrarySetting, Owner, Portable, SearchGlyph, SettingsError, unless_default,
};
use crate::settings_backup::settings_code;
use ibis_core::contract::{EngineError, ErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsRequest {
    pub appearance: Option<Appearance>,
    pub density: Option<Density>,
    pub larger_text: Option<bool>,
    pub paste: Option<bool>,
    pub search_glyph: Option<SearchGlyph>,
    pub shortcut: Option<ShortcutSetting>,
    pub library_file: Option<String>,
}

/// An omitted patch leaves the shortcut alone. An empty patch uses the default.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShortcutSetting {
    pub binding: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct SettingsResult {
    pub appearance: Appearance,
    pub density: Density,
    pub larger_text: bool,
    pub paste: bool,
    pub search_glyph: SearchGlyph,
    pub shortcut: Option<String>,
    pub library: Option<SettingsLibrary>,
    pub library_extensions: Vec<String>,
    pub version: &'static str,
}

#[derive(Serialize, JsonSchema)]
pub struct SettingsLibrary {
    pub name: String,
    pub file: Option<String>,
    /// A missing file is distinct from an unreadable source. This read does not open it.
    pub missing: bool,
}

impl SettingsRequest {
    pub fn answer(
        self,
        owner: &Owner,
        validate_file: impl FnOnce(&Path) -> Result<(), EngineError>,
    ) -> Result<SettingsResult, EngineError> {
        if self.appearance.is_none()
            && self.density.is_none()
            && self.larger_text.is_none()
            && self.paste.is_none()
            && self.search_glyph.is_none()
            && self.shortcut.is_none()
            && self.library_file.is_none()
        {
            let loaded = owner.read();
            return match loaded.problem {
                Some(problem) => Err(settings_code(&problem).into()),
                None => Ok(SettingsResult::from(&loaded.settings.portable)),
            };
        }

        let library = match self.library_file {
            Some(file) => {
                let path = PathBuf::from(file);
                if !path.is_absolute() {
                    return Err(ErrorCode::BadInput.into());
                }
                // The source engine can open a folder of PDFs too. This chooser accepts files.
                let candidate = path.clone();
                let metadata =
                    ibis_sources::inspect_io(move || candidate.metadata()).ok_or(ErrorCode::SourceUnavailable)?;
                if metadata.is_ok_and(|metadata| !metadata.is_file()) {
                    return Err(ErrorCode::BadInput.into());
                }
                validate_file(&path)?;
                Some(LibrarySetting::Bibtex(path))
            }
            None => None,
        };

        let (saved, ()) = owner
            .change(|settings| {
                let portable = &mut settings.portable;
                if let Some(appearance) = self.appearance {
                    portable.appearance = unless_default(appearance, Appearance::default());
                }
                if let Some(density) = self.density {
                    portable.density = unless_default(density, Density::default());
                }
                if let Some(larger_text) = self.larger_text {
                    portable.larger_text = unless_default(larger_text, DEFAULTS.larger_text);
                }
                if let Some(paste) = self.paste {
                    portable.paste = unless_default(paste, DEFAULTS.paste);
                }
                if let Some(glyph) = self.search_glyph {
                    portable.search_glyph = unless_default(glyph, SearchGlyph::default());
                }
                if let Some(shortcut) = self.shortcut {
                    portable.shortcut.set_here(shortcut.binding);
                }
                if let Some(library) = library {
                    portable.library = Some(library);
                }
                Ok::<(), SettingsError>(())
            })
            .map_err(|problem| settings_code(&problem))?;
        Ok(SettingsResult::from(&saved.portable))
    }
}

impl From<&Portable> for SettingsResult {
    fn from(portable: &Portable) -> Self {
        Self {
            appearance: portable.appearance(),
            density: portable.density(),
            larger_text: portable.larger_text(),
            paste: portable.paste(),
            search_glyph: portable.search_glyph(),
            shortcut: portable.shortcut.here().map(str::to_owned),
            library: portable.library.as_ref().map(SettingsLibrary::from),
            library_extensions: crate::library_file_extensions().map(str::to_owned).collect(),
            version: ibis_core::VERSION,
        }
    }
}

impl From<&LibrarySetting> for SettingsLibrary {
    fn from(library: &LibrarySetting) -> Self {
        match library {
            LibrarySetting::Bibtex(path) => {
                let probe = path.clone();
                Self {
                    name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                    file: Some(path.to_string_lossy().into_owned()),
                    missing: ibis_sources::inspect_io(move || probe.try_exists().ok()) == Some(Some(false)),
                }
            }
            LibrarySetting::Zotero(_) => Self {
                name: "Zotero".into(),
                file: None,
                missing: false,
            },
        }
    }
}
