//! Automatic suggestions only: eligibility never proves that a shortcut is free.
//! No activation loop: the caller must display/acknowledge each candidate first.

use tauri_plugin_global_shortcut::{Modifiers, Shortcut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Platform {
    Macos,
    Windows,
    Linux,
}

impl Platform {
    pub(super) fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

// Only existing defaults are approved. New fallbacks need an app-command review
// in addition to the static exclusions (#802); F13/F14 below are test fixtures.
pub(super) fn defaults(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::Macos => &["Control+Command+R"],
        Platform::Windows | Platform::Linux => &["Ctrl+Shift+Y"],
    }
}

// settings.md §2.3; conservative suggestion exclusions, never custom settings.
#[rustfmt::skip]
fn reserved(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::Macos => &[
            "Super+Space", "Control+Space", "Control+Alt+Space", "Super+Tab", "Super+Backquote",
            "Super+Shift+3", "Super+Shift+4", "Super+Shift+5", "Super+Shift+Space", "Control+ArrowUp",
            "Control+ArrowDown", "Control+ArrowLeft", "Control+ArrowRight", "Super+Alt+Escape",
            "Control+Super+Q", "Control+Super+F", "Control+Super+Space",
        ],
        Platform::Windows => &[
            "Super+L", "Super+D", "Super+E", "Super+R", "Super+I", "Super+V", "Super+Period", "Super+Space",
            "Super+Tab", "Super+0", "Super+1", "Super+2", "Super+3", "Super+4", "Super+5", "Super+6",
            "Super+7", "Super+8", "Super+9", "Super+Shift+S", "Control+Alt+Delete", "Control+Shift+Escape",
            "Alt+Tab", "Alt+F4", "Control+Escape", "F12",
        ],
        Platform::Linux => &[
            "Super+A", "Super+V", "Super+L", "Super+Space", "Alt+Tab", "Alt+F2", "Control+Alt+T", "PrintScreen",
            "Alt+Space", "Alt+F4", "Control+Alt+Delete",
        ],
    }
}

fn picker_keys(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::Macos => &["Super+K", "Super+Slash", "Super+Q", "Super+Comma", "Super+Shift+P"],
        _ => &[
            "Control+K",
            "Control+Slash",
            "Control+Q",
            "Control+Comma",
            "Control+Shift+P",
        ],
    }
}

fn normalized(platform: Platform, label: &str) -> Option<Shortcut> {
    label
        .split('+')
        .map(|part| match part.trim().to_ascii_lowercase().as_str() {
            "commandorcontrol" | "commandorctrl" | "cmdorctrl" | "cmdorcontrol" => {
                if platform == Platform::Macos {
                    "Super"
                } else {
                    "Control"
                }
            }
            _ => part.trim(),
        })
        .collect::<Vec<_>>()
        .join("+")
        .parse()
        .ok()
}

fn eligible(platform: Platform, label: &str) -> Option<Shortcut> {
    let shortcut = normalized(platform, label)?;
    let mods = shortcut.mods;
    let key = shortcut.key.to_string(); // Fixed physical Code, never the native error or input label.
    let key_allowed = key.strip_prefix("Key").is_some_and(|key| key.len() == 1)
        || key
            .strip_prefix('F')
            .and_then(|key| key.parse::<u8>().ok())
            .is_some_and(|key| (1..=24).contains(&key));
    // No bare/Shift-only keys, media/Fn keys, Option typing, AltGr or OS Super.
    // RegisterHotKey reserves F12 with any modifiers, not just bare F12.
    if !mods.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
        || mods.contains(Modifiers::ALT)
        || (platform != Platform::Macos && mods.contains(Modifiers::SUPER))
        || (platform == Platform::Windows && key == "F12")
        || !key_allowed
        || reserved(platform)
            .iter()
            .chain(picker_keys(platform))
            .any(|key| normalized(platform, key) == Some(shortcut))
        || (platform != Platform::Macos
            && ["Ctrl+Shift+R", "Ctrl+Shift+J"]
                .iter()
                .any(|key| normalized(platform, key) == Some(shortcut)))
        || (platform == Platform::Linux && normalized(platform, "Ctrl+Shift+U") == Some(shortcut))
    {
        return None;
    }
    Some(shortcut)
}

#[derive(Clone, Copy)]
pub(super) struct Candidate<'a> {
    pub(super) label: &'a str,
    pub(super) shortcut: Shortcut,
}

fn candidates_from<'a>(platform: Platform, labels: &[&'a str]) -> Vec<Candidate<'a>> {
    let mut candidates: Vec<Candidate<'a>> = Vec::new();
    for &label in labels {
        if let Some(shortcut) = eligible(platform, label)
            && !candidates.iter().any(|candidate| candidate.shortcut == shortcut)
        {
            candidates.push(Candidate { label, shortcut });
        }
    }
    candidates
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Attempt {
    Registered,
    RegistrationFailed,
    BackendUnavailable,
    Ineligible,
}

/// Pure progression over a finite reviewed list; never probes or registers keys.
pub(super) fn next_candidate<'a>(
    platform: Platform,
    labels: &[&'a str],
    previous: Option<(Shortcut, Attempt)>,
) -> Option<Candidate<'a>> {
    let candidates = candidates_from(platform, labels);
    let index = match previous {
        None => 0,
        Some((shortcut, Attempt::RegistrationFailed)) => {
            candidates.iter().position(|candidate| candidate.shortcut == shortcut)? + 1
        }
        _ => return None,
    };
    candidates.get(index).copied()
}

/// Attempt only the candidate the caller has already displayed/acknowledged.
pub(super) fn attempt<E>(
    platform: Platform,
    label: &str,
    backend: impl FnOnce() -> Result<(), ()>,
    register: impl FnOnce(Shortcut) -> Result<(), E>,
) -> Attempt {
    let Some(shortcut) = eligible(platform, label) else {
        return Attempt::Ineligible;
    };
    match super::register_with(backend, || register(shortcut)) {
        Ok(()) => Attempt::Registered,
        Err(super::SHORTCUT_BACKEND_UNAVAILABLE) => Attempt::BackendUnavailable,
        Err(_) => Attempt::RegistrationFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLATFORMS: [Platform; 3] = [Platform::Macos, Platform::Windows, Platform::Linux];

    fn fixtures(platform: Platform) -> [&'static str; 3] {
        if platform == Platform::Macos {
            ["Control+Command+R", "Control+Command+F13", "Control+Command+F14"]
        } else {
            ["Ctrl+Shift+Y", "Ctrl+Shift+F13", "Ctrl+Shift+F14"]
        }
    }

    #[test]
    fn every_profile_keeps_its_default_and_finite_order() {
        for platform in PLATFORMS {
            let expected = fixtures(platform);
            assert_eq!(defaults(platform), &expected[..1]);
            let mut previous = None;
            for label in expected {
                let candidate =
                    next_candidate(platform, &expected, previous).unwrap_or_else(|| panic!("pending fixture"));
                assert_eq!(candidate.label, label);
                assert_eq!(
                    candidate.shortcut,
                    label.parse::<Shortcut>().unwrap_or_else(|_| panic!("valid fixture"))
                );
                let outcome = attempt(platform, label, || Ok(()), |_| Err(()));
                assert_eq!(outcome, Attempt::RegistrationFailed);
                previous = Some((candidate.shortcut, outcome));
            }
            assert!(next_candidate(platform, &expected, previous).is_none());
            assert!(next_candidate(platform, &[], None).is_none());
        }
        let candidate = next_candidate(Platform::current(), defaults(Platform::current()), None)
            .unwrap_or_else(|| panic!("pending fixture"));
        assert_eq!(candidate.label, super::super::shortcut_label());
        assert_eq!(candidate.shortcut, super::super::default_shortcut());
    }

    #[test]
    fn aliases_are_normalized_before_eligibility_and_duplicate_rejection() {
        for platform in PLATFORMS {
            let labels = ["Shift+Control+KeyY", "ctrl+shift+y", "Ctrl+Shift+F13", "", "not a key"];
            let candidates = candidates_from(platform, &labels);
            assert_eq!(candidates.len(), 2);
            assert_eq!(candidates[0].label, labels[0]);
            assert_eq!(candidates[1].label, labels[2]);
            let command = if platform == Platform::Macos {
                "Super+K"
            } else {
                "Control+K"
            };
            assert!(eligible(platform, command).is_none());
            for alias in ["CmdOrCtrl", "CommandOrControl", "CommandOrCtrl", "CmdOrControl"] {
                assert!(eligible(platform, &format!("{alias}+K")).is_none());
            }
            assert_eq!(
                normalized(platform, "Ctrl+Command+R"),
                normalized(platform, "Super+Control+KeyR")
            );
        }
    }

    #[test]
    fn reserved_picker_and_editor_keys_never_reach_registration() {
        for platform in PLATFORMS {
            assert!(
                reserved(platform)
                    .iter()
                    .chain(picker_keys(platform))
                    .all(|label| normalized(platform, label).is_some())
            );
            for label in reserved(platform).iter().chain(picker_keys(platform)).copied().chain([
                "Escape",
                "Shift+Y",
                "Alt+Shift+Y",
                "Ctrl+Alt+Y",
                "Ctrl+Shift+Space",
                "Ctrl+Space",
                "Ctrl+AudioVolumeUp",
                "Ctrl+Fn",
                "Ctrl+Enter",
                "",
            ]) {
                assert!(eligible(platform, label).is_none(), "{platform:?}: {label}");
                assert_eq!(
                    attempt(
                        platform,
                        label,
                        || panic!("ineligible backend probe"),
                        |_| -> Result<(), ()> { panic!("ineligible registration") }
                    ),
                    Attempt::Ineligible
                );
            }
        }
        for platform in [Platform::Windows, Platform::Linux] {
            for label in ["Ctrl+Shift+R", "Ctrl+Shift+J"] {
                assert!(eligible(platform, label).is_none());
            }
        }
        for label in ["Ctrl+F12", "Ctrl+Shift+F12", "Super+F13"] {
            assert!(eligible(Platform::Windows, label).is_none());
        }
        assert!(eligible(Platform::Linux, "Ctrl+Shift+U").is_none());
    }

    #[test]
    fn each_attempt_registers_only_the_supplied_candidate() {
        for platform in PLATFORMS {
            let candidate =
                next_candidate(platform, &fixtures(platform), None).unwrap_or_else(|| panic!("pending fixture"));
            let calls = std::cell::Cell::new(0);
            let result = attempt(
                platform,
                candidate.label,
                || Ok(()),
                |shortcut| {
                    calls.set(calls.get() + 1);
                    assert_eq!(shortcut, candidate.shortcut);
                    Ok::<(), ()>(())
                },
            );
            assert_eq!(calls.get(), 1);
            assert_eq!(result, Attempt::Registered);
            assert!(next_candidate(platform, &fixtures(platform), Some((candidate.shortcut, result))).is_none());
            assert_eq!(
                attempt(
                    platform,
                    candidate.label,
                    || Err(()),
                    |_| -> Result<(), ()> { panic!("unavailable backend must not register") }
                ),
                Attempt::BackendUnavailable
            );
            let stopped = Some((candidate.shortcut, Attempt::BackendUnavailable));
            assert!(next_candidate(platform, &fixtures(platform), stopped).is_none());
        }
    }

    #[test]
    fn registration_failure_is_fixed_and_only_yields_a_pending_suggestion() {
        struct PrivateError;
        impl std::fmt::Display for PrivateError {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                panic!("private Display");
            }
        }
        impl std::fmt::Debug for PrivateError {
            fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                panic!("private Debug");
            }
        }
        for platform in PLATFORMS {
            let candidate =
                next_candidate(platform, &fixtures(platform), None).unwrap_or_else(|| panic!("pending fixture"));
            let result = attempt(platform, candidate.label, || Ok(()), |_| Err(PrivateError));
            assert_eq!(result, Attempt::RegistrationFailed);
            let pending = next_candidate(platform, &fixtures(platform), Some((candidate.shortcut, result)))
                .unwrap_or_else(|| panic!("pending fixture"));
            assert_ne!(pending.shortcut, candidate.shortcut);
            let unknown = "Ctrl+Shift+X".parse().unwrap_or_else(|_| panic!("valid fixture"));
            assert!(next_candidate(platform, &fixtures(platform), Some((unknown, result))).is_none());
        }
    }
}
