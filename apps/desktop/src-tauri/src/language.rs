//! The interface language: the person's choice in Settings, else the first of
//! the system's preferred languages Ibis speaks, else English. Citation
//! language is the style's or locale's own and never follows it.
use ibis_engine::catalogue::Language;

/// The language to speak, from the saved choice (`None` follows the system).
pub(crate) fn resolve(choice: Option<Language>) -> Language {
    choice.unwrap_or_else(|| {
        let preferred = system_languages();
        Language::negotiate(preferred.iter().map(String::as_str))
    })
}

/// The system's preferred languages, most preferred first.
#[cfg(target_os = "macos")]
fn system_languages() -> Vec<String> {
    // The per-app choice in System Settings › Language & Region lands here too.
    objc2_foundation::NSLocale::preferredLanguages()
        .iter()
        .map(|tag| tag.to_string())
        .collect()
}

#[cfg(target_os = "windows")]
fn system_languages() -> Vec<String> {
    let Ok(languages) = windows::System::UserProfile::GlobalizationPreferences::Languages() else {
        return Vec::new();
    };
    (0..languages.Size().unwrap_or_default())
        .filter_map(|index| languages.GetAt(index).ok())
        .map(|tag| tag.to_string_lossy())
        .collect()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn system_languages() -> Vec<String> {
    // gettext's order: LANGUAGE lists several, then the single locale settings.
    let mut tags: Vec<String> = std::env::var("LANGUAGE")
        .unwrap_or_default()
        .split(':')
        .map(str::to_owned)
        .collect();
    tags.extend(["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().filter_map(|name| std::env::var(name).ok()));
    tags.retain(|tag| !tag.is_empty());
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_choice_wins_over_the_system() {
        assert_eq!(resolve(Some(Language::Arabic)), Language::Arabic);
        assert_eq!(resolve(Some(Language::English)), Language::English);
    }
}
