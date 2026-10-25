//! Read-only discovery of configured Zotero data-directory candidates.
//!
//! Roots are supplied by the caller: this module never inspects the environment,
//! opens a database, executes preferences or writes files. Candidates need not
//! exist and must be validated by the offline reader before use. Multiple
//! candidates require an explicit choice; discovery does not select a library.
//! Missing profile configuration falls back to `home/Zotero`. Legacy profile
//! data and interrupted migrations require manual override; no database or
//! migration marker is inspected here. See Zotero's directory selection:
//! <https://github.com/zotero/zotero/blob/main/chrome/content/zotero/xpcom/dataDirectory.js>.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

mod preferences;
#[cfg(all(test, unix))]
mod regular_file_tests;

const MAX_REGISTRY_BYTES: u64 = 64 * 1024;
const MAX_PREFERENCES_BYTES: u64 = 1024 * 1024;
const MAX_PROFILES: usize = 64;

/// Platform whose Zotero profile layout is being discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

/// Caller-supplied roots; Debug deliberately omits personal paths.
#[derive(Clone)]
pub struct DiscoveryRoots {
    platform: Platform,
    home: PathBuf,
    profiles: PathBuf,
}

impl DiscoveryRoots {
    /// Use the platform's documented profile registry root. Windows honors a
    /// supplied roaming AppData root, otherwise `home/AppData/Roaming`.
    /// All supplied roots must be absolute on the host performing discovery.
    pub fn new(platform: Platform, home: PathBuf, roaming_app_data: Option<PathBuf>) -> Self {
        let profiles = match platform {
            Platform::MacOs => home.join("Library/Application Support/Zotero"),
            Platform::Windows => roaming_app_data
                .unwrap_or_else(|| home.join("AppData/Roaming"))
                .join("Zotero/Zotero"),
            Platform::Linux => home.join(".zotero/zotero"),
        };
        Self {
            platform,
            home,
            profiles,
        }
    }
}

impl fmt::Debug for DiscoveryRoots {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoveryRoots")
            .field("platform", &self.platform)
            .field("paths", &"[redacted]")
            .finish()
    }
}

/// How a candidate was found; this does not establish database availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Manual,
    Preferences,
    Default,
}

/// A path for explicit downstream use, never included in Debug output.
#[derive(Clone, PartialEq, Eq)]
pub struct DataDirectory {
    path: PathBuf,
    origin: Origin,
}

impl DataDirectory {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn origin(&self) -> Origin {
        self.origin
    }
}

impl fmt::Debug for DataDirectory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataDirectory")
            .field("path", &"[redacted]")
            .field("origin", &self.origin)
            .finish()
    }
}

/// Fixed diagnostics: neither raw I/O errors nor configuration text escape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warning {
    InvalidPath,
    UnreadableConfiguration,
    NotRegularFile,
    ConfigurationTooLarge,
    InvalidProfiles,
    TooManyProfiles,
    UnsupportedPreferences,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPath => "Zotero discovery requires an absolute directory path.",
            Self::UnreadableConfiguration => "Zotero profile configuration could not be read.",
            Self::NotRegularFile => "Zotero profile configuration is not a regular file.",
            Self::ConfigurationTooLarge => "Zotero profile configuration exceeds the size limit.",
            Self::InvalidProfiles => "Zotero profile registry is invalid.",
            Self::TooManyProfiles => "Zotero profile registry exceeds the profile limit.",
            Self::UnsupportedPreferences => {
                "Zotero preferences are invalid or unsupported; choose a directory manually."
            }
        })
    }
}

/// Unique lexical candidates in registry order, plus bounded fixed warnings.
/// No canonicalization, directory enumeration or candidate filesystem access.
#[derive(Debug, Default)]
pub struct Discovery {
    pub directories: Vec<DataDirectory>,
    pub warnings: Vec<Warning>,
}

impl Discovery {
    fn add(&mut self, path: PathBuf, origin: Origin) {
        if !self.directories.iter().any(|candidate| candidate.path == path) {
            self.directories.push(DataDirectory { path, origin });
        }
    }

    fn warn(&mut self, warning: Warning) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }
}

/// Discover configured candidates without probing their contents. An absolute
/// manual override is returned alone and bypasses all profile reads, even when
/// the roots are unavailable. Bad configuration never silently becomes default.
pub fn discover(roots: &DiscoveryRoots, manual: Option<&Path>) -> Discovery {
    let mut result = Discovery::default();
    if let Some(path) = manual {
        if absolute(path, roots.platform) {
            result.add(path.to_path_buf(), Origin::Manual);
        } else {
            result.warn(Warning::InvalidPath);
        }
        return result;
    }
    if !roots.home.is_absolute() || !roots.profiles.is_absolute() {
        result.warn(Warning::InvalidPath);
        return result;
    }
    let registry = read_text(&roots.profiles.join("profiles.ini"), MAX_REGISTRY_BYTES);
    let profiles = match registry.and_then(|text| text.map(|text| profile_paths(&text, roots)).transpose()) {
        Ok(Some(profiles)) => profiles,
        Ok(None) => {
            result.add(roots.home.join("Zotero"), Origin::Default);
            return result;
        }
        Err(warning) => {
            result.warn(warning);
            return result;
        }
    };
    for profile in profiles {
        match read_text(&profile.join("prefs.js"), MAX_PREFERENCES_BYTES).and_then(|text| {
            preferences::data_directory(text.as_deref().unwrap_or_default())
                .map_err(|()| Warning::UnsupportedPreferences)
        }) {
            Ok(Some(path)) if absolute(Path::new(&path), roots.platform) => {
                result.add(PathBuf::from(path), Origin::Preferences)
            }
            Ok(Some(_)) => result.warn(Warning::InvalidPath),
            Ok(None) => result.add(roots.home.join("Zotero"), Origin::Default),
            Err(warning) => result.warn(warning),
        }
    }
    result
}

fn absolute(path: &Path, platform: Platform) -> bool {
    if path.as_os_str().as_encoded_bytes().iter().any(u8::is_ascii_control) {
        return false;
    }
    let Some(text) = path.to_str() else {
        return path.is_absolute();
    };
    if text.chars().any(char::is_control) {
        return false;
    }
    match platform {
        Platform::Windows => {
            let extended = text.starts_with("\\\\?\\");
            let text = text.strip_prefix("\\\\?\\").unwrap_or(text);
            let bytes = text.as_bytes();
            let unc = if extended {
                text.strip_prefix("UNC\\")
            } else {
                text.strip_prefix("\\\\").or_else(|| text.strip_prefix("//"))
            };
            if let Some(unc) = unc {
                let mut parts = unc.split(['\\', '/']);
                let server = parts.next().unwrap_or_default();
                return !matches!(server, "" | "." | "?") && parts.next().is_some_and(|share| !share.is_empty());
            }
            (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'))
                || (!extended && path.is_absolute()) // Injected fixtures may use host paths.
        }
        _ => text.starts_with('/') || path.is_absolute(),
    }
}

fn read_text(path: &Path, limit: u64) -> Result<Option<String>, Warning> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Warning::UnreadableConfiguration),
    };
    if !metadata.is_file() {
        return Err(Warning::NotRegularFile);
    }
    if metadata.len() > limit {
        return Err(Warning::ConfigurationTooLarge);
    }
    let file = open_configuration(path)?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Warning::UnreadableConfiguration)?;
    if bytes.len() as u64 > limit {
        return Err(Warning::ConfigurationTooLarge);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| Warning::UnreadableConfiguration)
}

fn open_configuration(path: &Path) -> Result<File, Warning> {
    let mut options = OpenOptions::new();
    options.read(true);
    // Do not follow a replaced final symlink, or wait for a FIFO writer between
    // the path check and open. ABI flags avoid an additional dependency.
    // Darwin: apple-oss-distributions/xnu bsd/sys/fcntl.h.
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        const O_NONBLOCK: i32 = 0x4;
        const O_NOFOLLOW: i32 = 0x100;
        options.custom_flags(O_NONBLOCK | O_NOFOLLOW);
    }
    // Linux asm-generic/fcntl.h, used by the supported x86-64/ARM64 targets.
    #[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        const O_NONBLOCK: i32 = 0x800;
        const O_NOFOLLOW: i32 = 0x20000;
        options.custom_flags(O_NONBLOCK | O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|_| Warning::UnreadableConfiguration)?;
    let metadata = file.metadata().map_err(|_| Warning::UnreadableConfiguration)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Warning::NotRegularFile);
    }
    Ok(file)
}

fn profile_paths(text: &str, roots: &DiscoveryRoots) -> Result<Vec<PathBuf>, Warning> {
    let mut profiles = Vec::new();
    let mut fields = None;
    for line in text.trim_start_matches('\u{feff}').split(['\r', '\n']).chain(["[end]"]) {
        let line = line.trim();
        if line.is_empty() || line.starts_with([';', '#']) {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if let Some((relative, path)) = fields.take() {
                let (relative, path): (Option<&str>, Option<&str>) = (relative, path);
                let path = path.ok_or(Warning::InvalidProfiles)?;
                let path = match relative {
                    Some("1")
                        if !path.is_empty()
                            && !path.contains('\\')
                            && !path.contains(':')
                            && !path.chars().any(char::is_control)
                            && Path::new(path).components().all(|c| matches!(c, Component::Normal(_))) =>
                    {
                        roots.profiles.join(path)
                    }
                    Some("0") if absolute(Path::new(path), roots.platform) => PathBuf::from(path),
                    _ => return Err(Warning::InvalidProfiles),
                };
                if !profiles.contains(&path) {
                    profiles.push(path);
                }
                if profiles.len() > MAX_PROFILES {
                    return Err(Warning::TooManyProfiles);
                }
            }
            fields = section
                .strip_prefix("Profile")
                .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                .map(|_| (None, None));
        } else if let Some((key, value)) = line.split_once('=') {
            if let Some((relative, path)) = fields.as_mut() {
                let field = match key.trim() {
                    "IsRelative" => Some(relative),
                    "Path" => Some(path),
                    _ => None,
                };
                if let Some(field) = field
                    && field.replace(value.trim()).is_some()
                {
                    return Err(Warning::InvalidProfiles);
                }
            }
        } else {
            return Err(Warning::InvalidProfiles);
        }
    }
    if profiles.is_empty() {
        Err(Warning::InvalidProfiles)
    } else {
        Ok(profiles)
    }
}
