//! Material policy is independent of a window so every fallback is testable.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Appearance {
    #[default]
    Opaque,
    #[cfg(any(target_os = "windows", test))]
    Mica,
    #[cfg(any(target_os = "macos", test))]
    Vibrancy,
}

#[derive(Clone, Copy)]
pub(super) struct Preferences {
    pub(super) high_contrast: bool,
    pub(super) reduce_transparency: bool,
}

pub(super) fn requested(material: Appearance, preferences: Option<Preferences>) -> Appearance {
    match preferences {
        Some(Preferences {
            high_contrast: false,
            reduce_transparency: false,
        }) => material,
        _ => Appearance::Opaque,
    }
}

pub(super) fn applied(material: Appearance, success: bool) -> Appearance {
    if success { material } else { Appearance::Opaque }
}

/// The evidence log accepts only native policy values, never an error payload.
pub(super) fn evidence_reason(
    material: Appearance,
    preferences: Option<Preferences>,
    transparent: bool,
    actual: Appearance,
) -> &'static str {
    if material == Appearance::Opaque {
        return "unsupported";
    }
    if !transparent {
        return "not-transparent";
    }
    let Some(preferences) = preferences else {
        return "unknown-preferences";
    };
    if preferences.high_contrast || preferences.reduce_transparency {
        return "accessibility";
    }
    if actual != material {
        return "native-failure";
    }
    "applied"
}

impl Appearance {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            #[cfg(any(target_os = "windows", test))]
            Self::Mica => "mica",
            #[cfg(any(target_os = "macos", test))]
            Self::Vibrancy => "vibrancy",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_distinguishes_native_success_from_every_opaque_fallback() {
        let normal = Some(Preferences {
            high_contrast: false,
            reduce_transparency: false,
        });
        let accessible = Some(Preferences {
            high_contrast: false,
            reduce_transparency: true,
        });
        assert_eq!(
            evidence_reason(Appearance::Opaque, None, false, Appearance::Opaque),
            "unsupported"
        );
        assert_eq!(
            evidence_reason(Appearance::Mica, normal, false, Appearance::Opaque),
            "not-transparent"
        );
        assert_eq!(
            evidence_reason(Appearance::Mica, None, true, Appearance::Opaque),
            "unknown-preferences"
        );
        assert_eq!(
            evidence_reason(Appearance::Vibrancy, accessible, true, Appearance::Opaque),
            "accessibility"
        );
        assert_eq!(
            evidence_reason(Appearance::Mica, normal, true, Appearance::Opaque),
            "native-failure"
        );
        assert_eq!(
            evidence_reason(Appearance::Vibrancy, normal, true, Appearance::Vibrancy),
            "applied"
        );
    }

    #[test]
    fn unknown_preferences_are_opaque_on_every_platform() {
        for material in [Appearance::Opaque, Appearance::Mica, Appearance::Vibrancy] {
            assert_eq!(requested(material, None), Appearance::Opaque);
        }
    }

    #[test]
    fn either_accessibility_preference_requires_opacity() {
        for material in [Appearance::Mica, Appearance::Vibrancy] {
            for (high_contrast, reduce_transparency) in [(true, false), (false, true), (true, true)] {
                assert_eq!(
                    requested(
                        material,
                        Some(Preferences {
                            high_contrast,
                            reduce_transparency
                        })
                    ),
                    Appearance::Opaque
                );
            }
        }
    }

    #[test]
    fn only_supported_materials_and_explicit_preferences_allow_effects() {
        let preferences = || {
            Some(Preferences {
                high_contrast: false,
                reduce_transparency: false,
            })
        };
        assert_eq!(requested(Appearance::Opaque, preferences()), Appearance::Opaque);
        assert_eq!(requested(Appearance::Mica, preferences()), Appearance::Mica);
        assert_eq!(requested(Appearance::Vibrancy, preferences()), Appearance::Vibrancy);
    }

    #[test]
    fn native_failure_never_exposes_a_transparent_surface() {
        for material in [Appearance::Opaque, Appearance::Mica, Appearance::Vibrancy] {
            assert_eq!(applied(material, false), Appearance::Opaque);
            assert_eq!(applied(material, true), material);
        }
        assert_eq!(Appearance::Opaque.name(), "opaque");
    }
}
