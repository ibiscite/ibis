//! Where a citation goes: the output for the app the picker was opened from,
//! and whether pasting into that app is safe.
//!
//! Both are pure decisions on data the platform layer captures, so the app,
//! the command line and agents agree (docs/architecture.md, "Pasting into the
//! previous app"). Every refusal still leaves the citation on the clipboard,
//! with a [`CopyReason`] the front end words for people.

mod table;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::contract::citation::{KeySyntax, OutputSpec};

pub use table::{APPS, App};

/// An app ID that cannot tell Ibis which output its document needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "type-export", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum OutputQuestion {
    Browser,
    Editor,
}

/// The operating system an app ID belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum System {
    Macos,
    Windows,
    Linux,
}

/// An app as the platform names it: a bundle ID on macOS, an executable name
/// on Windows. Never a window title or document name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AppId {
    pub system: System,
    pub id: String,
}

impl AppId {
    /// The form the table uses: bundle IDs compare without case, as macOS
    /// does; a Windows executable is its lower-case file name, whatever path
    /// the platform reported.
    pub fn normalized(&self) -> String {
        match self.system {
            System::Windows => self
                .id
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase(),
            System::Macos | System::Linux => self.id.to_ascii_lowercase(),
        }
    }
}

/// What kind of citation an app wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OutputClass {
    /// Word processors and mail: the formatted citation in the default style.
    Formatted,
    /// A citation key syntax the app's documents use.
    Key(KeySyntax),
}

/// How safe it is to type into an app for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PasteSafety {
    Normal,
    /// A paste runs as typed input in a shell.
    Terminal,
    /// Keystrokes go to another computer, whose clipboard may differ.
    RemoteDesktop,
    /// Keystrokes go to a guest system, whose clipboard may differ.
    VirtualMachine,
}

/// The app the picker was opened from, as captured before the panel showed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub system: System,
    /// `None` when capture failed or no app was in front.
    pub app: Option<AppId>,
    /// The focused field is known to be a password field, for example from
    /// Windows UI Automation `IsPassword`. A system-wide secure-input flag
    /// cannot establish this and uses a separate copy reason.
    #[serde(default)]
    pub secure_field: bool,
    /// The app runs with more rights than Ibis (Windows integrity level), so
    /// the system would drop the keystrokes.
    #[serde(default)]
    pub elevated: bool,
}

/// What the user and the system allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PastePermission {
    /// The "paste into the app" switch (on by default).
    pub enabled: bool,
    /// The system lets Ibis send the paste keystroke (macOS PostEvent).
    pub granted: bool,
}

/// Why a citation was copied instead of pasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CopyReason {
    PasteOff,
    UnsupportedSystem,
    NoTarget,
    /// The target's paste shortcut has not been verified.
    UnknownApp,
    TargetChanged,
    ClipboardChanged,
    PasteExpired,
    SecureField,
    /// System-wide secure typing is enabled, possibly by another app.
    SecureInput,
    /// The final paste check was unavailable or could not finish in time.
    PasteUnavailable,
    ElevatedApp,
    Terminal,
    RemoteDesktop,
    VirtualMachine,
    NoPermission,
    /// macOS Accessibility trust is required for the identity-only focus proof.
    AccessibilityRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", content = "reason", rename_all = "kebab-case")]
pub enum Decision {
    Paste,
    Copy(CopyReason),
}

/// The table entry for an app, if Ibis knows it.
pub fn lookup(app: &AppId) -> Option<&'static App> {
    let id = app.normalized();
    APPS.iter().find(|entry| entry.system == app.system && entry.id == id)
}

/// Paste, or copy with the reason. Checks run from the user's own choice to
/// what the target app is; the first that applies wins.
pub fn decide(target: &Target, permission: PastePermission) -> Decision {
    let reason = if !permission.enabled {
        CopyReason::PasteOff
    } else if target.system == System::Linux {
        CopyReason::UnsupportedSystem
    } else if target.app.as_ref().is_none_or(|app| app.system != target.system) {
        CopyReason::NoTarget
    } else if target.secure_field {
        CopyReason::SecureField
    } else if target.elevated {
        CopyReason::ElevatedApp
    } else {
        match target.app.as_ref().and_then(lookup).map(|app| app.safety) {
            Some(PasteSafety::Terminal) => CopyReason::Terminal,
            Some(PasteSafety::RemoteDesktop) => CopyReason::RemoteDesktop,
            Some(PasteSafety::VirtualMachine) => CopyReason::VirtualMachine,
            None => CopyReason::UnknownApp,
            Some(PasteSafety::Normal) if !permission.granted => CopyReason::NoPermission,
            Some(PasteSafety::Normal) => return Decision::Paste,
        }
    };
    Decision::Copy(reason)
}

/// The output for `app`: its class from the table, or `default` for an app
/// Ibis does not know. Formatted apps use `style`, the default style, even
/// when the default output is a key syntax.
pub fn output_for(app: Option<&AppId>, default: &OutputSpec, style: &str) -> OutputSpec {
    match app.and_then(lookup).and_then(|app| app.output) {
        Some(OutputClass::Formatted) => OutputSpec::Csl {
            style: style.to_owned(),
        },
        Some(OutputClass::Key(syntax)) => OutputSpec::Key { syntax, command: None },
        None => default.clone(),
    }
}

#[cfg(test)]
mod tests;
