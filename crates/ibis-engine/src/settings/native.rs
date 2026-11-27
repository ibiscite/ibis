use super::{Appearance, DEFAULTS, Density, Owner, Portable, SettingsError, unless_default};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsRequest {
    pub appearance: Option<Appearance>,
    pub density: Option<Density>,
    pub larger_text: Option<bool>,
}

#[derive(Serialize, JsonSchema)]
pub struct SettingsResult {
    pub appearance: Appearance,
    pub density: Density,
    pub larger_text: bool,
    pub version: &'static str,
}

impl SettingsRequest {
    pub fn answer(self, owner: &Owner) -> Result<SettingsResult, SettingsError> {
        if self.appearance.is_none() && self.density.is_none() && self.larger_text.is_none() {
            let loaded = owner.read();
            return match loaded.problem {
                Some(problem) => Err(problem),
                None => Ok(SettingsResult::from(&loaded.settings.portable)),
            };
        }

        let (saved, ()) = owner.change(|settings| {
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
            Ok::<(), SettingsError>(())
        })?;
        Ok(SettingsResult::from(&saved.portable))
    }
}

impl From<&Portable> for SettingsResult {
    fn from(portable: &Portable) -> Self {
        Self {
            appearance: portable.appearance(),
            density: portable.density(),
            larger_text: portable.larger_text(),
            version: ibis_core::VERSION,
        }
    }
}
