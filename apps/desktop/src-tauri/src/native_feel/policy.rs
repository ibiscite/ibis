//! Material policy is independent of a window so every fallback is testable.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Appearance {
    #[default]
    Opaque,
    #[cfg(any(target_os = "windows", test))]
    Acrylic,
    #[cfg(any(target_os = "macos", test))]
    Vibrancy,
    #[cfg(any(target_os = "macos", test))]
    LiquidGlass,
}

#[derive(Clone, Copy)]
pub(super) struct Preferences {
    pub(super) high_contrast: bool,
    pub(super) reduce_transparency: bool,
}

/// Only the documented DWM Acrylic path has a checked result. Unknown OS
/// versions must not reach the library's legacy, unchecked composition call.
#[cfg(any(target_os = "windows", test))]
pub(super) fn windows_material(version: Option<&str>) -> Appearance {
    let version = version
        .filter(|v| !v.is_empty() && v.len() <= 20 && v.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|v| v.parse::<u64>().ok());
    match version {
        Some(v) if v >> 48 == 10 && (v >> 32) & 0xffff == 0 && (v >> 16) & 0xffff >= 22621 => Appearance::Acrylic,
        _ => Appearance::Opaque,
    }
}

#[cfg(any(target_os = "windows", test))]
pub(super) fn energy_saver_allows_effects(status: i32) -> Option<bool> {
    match status {
        0 | 1 => Some(true), // Disabled or Off.
        2 => Some(false),    // On.
        _ => None,
    }
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

#[cfg(any(target_os = "macos", test))]
pub(super) enum GlassFailure {
    Unsupported,
    Failed,
}

/// An unsupported OS permits vibrancy; other failures must stay opaque.
#[cfg(any(target_os = "macos", test))]
pub(super) fn macos_material(
    glass: Result<(), GlassFailure>,
    vibrancy: impl FnOnce() -> Result<(), ()>,
) -> Result<Appearance, ()> {
    match glass {
        Ok(()) => Ok(Appearance::LiquidGlass),
        Err(GlassFailure::Unsupported) => vibrancy().map(|()| Appearance::Vibrancy),
        Err(GlassFailure::Failed) => Err(()),
    }
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
    #[cfg(any(target_os = "macos", test))]
    if material == Appearance::LiquidGlass && actual == Appearance::Vibrancy {
        return "unsupported-glass";
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
            Self::Acrylic => "acrylic",
            #[cfg(any(target_os = "macos", test))]
            Self::Vibrancy => "vibrancy",
            #[cfg(any(target_os = "macos", test))]
            Self::LiquidGlass => "liquid-glass",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_uses_glass_or_only_the_supported_vibrancy_fallback() {
        assert_eq!(
            macos_material(Ok(()), || panic!("glass needs no fallback")),
            Ok(Appearance::LiquidGlass)
        );
        assert_eq!(
            macos_material(Err(GlassFailure::Unsupported), || Ok(())),
            Ok(Appearance::Vibrancy)
        );
        assert_eq!(macos_material(Err(GlassFailure::Unsupported), || Err(())), Err(()));
        assert_eq!(
            macos_material(Err(GlassFailure::Failed), || panic!(
                "native failures must not try fallback"
            )),
            Err(())
        );
        let normal = Some(Preferences {
            high_contrast: false,
            reduce_transparency: false,
        });
        assert_eq!(
            evidence_reason(Appearance::LiquidGlass, normal, true, Appearance::LiquidGlass),
            "applied"
        );
        assert_eq!(
            evidence_reason(Appearance::LiquidGlass, normal, true, Appearance::Vibrancy),
            "unsupported-glass"
        );
        assert_eq!(
            evidence_reason(Appearance::LiquidGlass, normal, true, Appearance::Opaque),
            "native-failure"
        );
    }

    #[test]
    fn acrylic_requires_the_documented_windows_build_and_known_energy_policy() {
        for build in [0_u64, 22000, 22523, 22620, 22621, 26100, 65535] {
            let version = ((10_u64 << 48) | (build << 16)).to_string();
            assert_eq!(
                windows_material(Some(&version)),
                if build >= 22621 {
                    Appearance::Acrylic
                } else {
                    Appearance::Opaque
                }
            );
        }
        for version in [
            None,
            Some(""),
            Some("-1"),
            Some("+2814751249604608"),
            Some("18446744073709551616"),
            Some("10.0.26100"),
            Some(" 2814751249604608"),
        ] {
            assert_eq!(windows_material(version), Appearance::Opaque);
        }
        for version in [
            (11_u64 << 48) | (26100 << 16),
            (10_u64 << 48) | (1 << 32) | (26100 << 16),
        ] {
            assert_eq!(windows_material(Some(&version.to_string())), Appearance::Opaque);
        }
        for (status, expected) in [
            (-1, None),
            (0, Some(true)),
            (1, Some(true)),
            (2, Some(false)),
            (3, None),
        ] {
            assert_eq!(energy_saver_allows_effects(status), expected);
        }
    }

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
            evidence_reason(Appearance::Acrylic, normal, false, Appearance::Opaque),
            "not-transparent"
        );
        assert_eq!(
            evidence_reason(Appearance::Acrylic, None, true, Appearance::Opaque),
            "unknown-preferences"
        );
        assert_eq!(
            evidence_reason(Appearance::Vibrancy, accessible, true, Appearance::Opaque),
            "accessibility"
        );
        assert_eq!(
            evidence_reason(Appearance::Acrylic, normal, true, Appearance::Opaque),
            "native-failure"
        );
        assert_eq!(
            evidence_reason(Appearance::Vibrancy, normal, true, Appearance::Vibrancy),
            "applied"
        );
    }

    #[test]
    fn unknown_preferences_are_opaque_on_every_platform() {
        for material in [
            Appearance::Opaque,
            Appearance::Acrylic,
            Appearance::Vibrancy,
            Appearance::LiquidGlass,
        ] {
            assert_eq!(requested(material, None), Appearance::Opaque);
        }
    }

    #[test]
    fn either_accessibility_preference_requires_opacity() {
        for material in [Appearance::Acrylic, Appearance::Vibrancy, Appearance::LiquidGlass] {
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
        assert_eq!(requested(Appearance::Acrylic, preferences()), Appearance::Acrylic);
        assert_eq!(requested(Appearance::Vibrancy, preferences()), Appearance::Vibrancy);
    }

    #[test]
    fn native_material_names_are_fixed() {
        assert_eq!(Appearance::Opaque.name(), "opaque");
        assert_eq!(Appearance::Acrylic.name(), "acrylic");
        assert_eq!(Appearance::Vibrancy.name(), "vibrancy");
        assert_eq!(Appearance::LiquidGlass.name(), "liquid-glass");
    }
}
