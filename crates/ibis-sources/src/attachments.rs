//! File-field decoding. Locations stay in the source owner, never the snapshot.
use crate::{SourceWarning, WarningCode};
use ibis_core::contract::{Attachment, Availability, ItemRef, LibraryObject};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) fn append(
    base: &Path,
    fields: Vec<(ItemRef, String, Option<usize>)>,
    objects: &mut Vec<LibraryObject>,
    paths: &mut BTreeMap<ItemRef, PathBuf>,
    warnings: &mut Vec<SourceWarning>,
) {
    append_with(base, fields, objects, paths, warnings, availability);
}

fn append_with(
    base: &Path,
    fields: Vec<(ItemRef, String, Option<usize>)>,
    objects: &mut Vec<LibraryObject>,
    paths: &mut BTreeMap<ItemRef, PathBuf>,
    warnings: &mut Vec<SourceWarning>,
    mut probe: impl FnMut(&Path) -> Availability,
) {
    for (parent, field, line) in fields {
        for (index, encoded) in split_escaped(&field, ';').into_iter().enumerate() {
            if encoded.trim().is_empty() {
                continue;
            }
            let id = ItemRef {
                library: parent.library.clone(),
                key: format!("attachment:{}:{index}", parent.key),
            };
            let parsed = decode(&encoded);
            let (title, media_type, path, availability) = match parsed {
                Some((title, location, kind))
                    if location.starts_with("https://") || location.starts_with("http://") =>
                {
                    (title, kind, None, Availability::Online)
                }
                Some((title, location, kind)) => {
                    let relative = Path::new(&location);
                    // Explicit network locations are never touched implicitly.
                    if location.starts_with("\\\\")
                        || location.starts_with("//")
                        || location.contains("://")
                        || cfg!(not(windows)) && windows_drive(&location)
                    {
                        warnings.push(SourceWarning {
                            code: WarningCode::InvalidAttachment,
                            line,
                        });
                        (title, kind, None, Availability::Unknown)
                    } else {
                        let path = if relative.is_absolute() {
                            relative.to_path_buf()
                        } else {
                            base.join(relative)
                        };
                        let availability = probe(&path);
                        (title, kind, Some(path), availability)
                    }
                }
                None => {
                    warnings.push(SourceWarning {
                        code: WarningCode::InvalidAttachment,
                        line,
                    });
                    (String::new(), String::new(), None, Availability::Unknown)
                }
            };
            if let Some(path) = path {
                paths.insert(id.clone(), path);
            }
            objects.push(LibraryObject::Attachment(Attachment {
                id,
                parent: Some(parent.clone()),
                title,
                media_type,
                availability,
                content_hash: None,
            }));
        }
    }
}

fn windows_drive(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'/' | b'\\')
}
fn decode(text: &str) -> Option<(String, String, String)> {
    let text = text.trim();
    if text.chars().any(char::is_control) {
        return None;
    }
    if text.starts_with("https://") || text.starts_with("http://") {
        return Some((String::new(), text.into(), String::new()));
    }
    if windows_drive(text) {
        return Some((String::new(), text.into(), mime(text)));
    }
    let fields = split_escaped(text, ':');
    let (title, location, media_type) = match fields.as_slice() {
        [path] => (String::new(), path.trim().to_owned(), mime(path)),
        [title, path, kind] if is_media_type(kind) => {
            (title.trim().to_owned(), path.trim().to_owned(), normalize_media(kind))
        }
        [path, kind] if is_media_type(kind) => (String::new(), path.trim().to_owned(), normalize_media(kind)),
        _ => return None,
    };
    if location.is_empty() {
        None
    } else {
        Some((title, location, media_type))
    }
}
fn is_media_type(text: &str) -> bool {
    let text = text.trim();
    matches!(text.to_ascii_lowercase().as_str(), "pdf" | "html" | "epub" | "txt")
        || text.contains('/')
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/-+._".contains(&byte))
}
fn normalize_media(text: &str) -> String {
    match text.trim().to_ascii_lowercase().as_str() {
        "pdf" => "application/pdf".into(),
        "html" => "text/html".into(),
        "epub" => "application/epub+zip".into(),
        "txt" => "text/plain".into(),
        other => other.into(),
    }
}
fn mime(path: &str) -> String {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("html" | "htm") => "text/html",
        Some("epub") => "application/epub+zip",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
    .into()
}
// Only delimiter escapes are decoded. Ordinary Windows backslashes remain literal.
fn split_escaped(text: &str, delimiter: char) -> Vec<String> {
    let mut values = vec![String::new()];
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some_and(|next| *next == delimiter) {
            if let Some(next) = chars.next()
                && let Some(value) = values.last_mut()
            {
                value.push(next);
            }
        } else if c == delimiter {
            values.push(String::new());
        } else if let Some(value) = values.last_mut() {
            value.push(c);
        }
    }
    values
}
fn availability(path: &Path) -> Availability {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return match error.kind() {
                std::io::ErrorKind::NotFound => Availability::BrokenLink,
                std::io::ErrorKind::PermissionDenied => Availability::Unreadable,
                _ => Availability::Unknown,
            };
        }
    };
    if !metadata.is_file() {
        return Availability::Unknown;
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        if metadata.st_flags() & 0x4000_0000 != 0 {
            return Availability::CloudPlaceholder;
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & (0x1000 | 0x0004_0000 | 0x0040_0000) != 0 {
            return Availability::CloudPlaceholder;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o444 == 0 {
            return Availability::Unreadable;
        }
    }
    Availability::OnDisk
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_paths_and_jabref_fields_keep_spaces_and_escaped_delimiters() {
        assert_eq!(
            decode("one  two.pdf"),
            Some((String::new(), "one  two.pdf".into(), "application/pdf".into()))
        );
        assert_eq!(
            decode(r"Label:C\:\\synthetic.pdf:PDF"),
            Some(("Label".into(), r"C:\\synthetic.pdf".into(), "application/pdf".into()))
        );
        assert!(decode("ambiguous:thing:else").is_none());
        assert_eq!(
            split_escaped(r"one\;two.pdf;three.pdf", ';'),
            ["one;two.pdf", "three.pdf"]
        );
    }
}

#[cfg(test)]
mod adversarial_tests {
    use super::*;
    use ibis_core::contract::LibraryId;

    fn check(text: &str) -> (Vec<LibraryObject>, usize, Vec<SourceWarning>) {
        let parent = ItemRef {
            library: LibraryId::file(b"invented"),
            key: "record:one".into(),
        };
        let mut objects = Vec::new();
        let mut paths = BTreeMap::new();
        let mut warnings = Vec::new();
        let mut probes = 0;
        append_with(
            Path::new("/invented"),
            vec![(parent, text.into(), Some(2))],
            &mut objects,
            &mut paths,
            &mut warnings,
            |_| {
                probes += 1;
                Availability::BrokenLink
            },
        );
        (objects, probes, warnings)
    }

    #[test]
    fn remote_windows_prefixes_never_reach_metadata() {
        for location in [
            r"\/attacker.example/share/x.pdf",
            r"/\host\share\x.pdf",
            r"\\host\share\x.pdf",
            "//host/share/x.pdf",
            r"\\?\UNC\host\share\x.pdf",
            r"\\?\C:\invented.pdf",
            r"\\.\pipe\invented",
            r"/\?\UNC\host\share\x.pdf",
            r"\??\UNC\host\share\x.pdf",
            "/??/UNC/host/share/x.pdf",
            "file://host/share/x.pdf",
            "FILE:///invented/private.pdf",
            r"file:\\host\share\x.pdf",
            "C:invented.pdf",
        ] {
            let (_, probes, warnings) = check(location);
            assert_eq!(probes, 0, "unsupported location must not be probed: {location}");
            assert!(
                warnings
                    .iter()
                    .any(|warning| warning.code == WarningCode::InvalidAttachment)
            );
        }
    }

    #[test]
    fn paths_cannot_become_media_types() {
        for text in [
            "file:///invented/private.pdf",
            "synthetic.pdf:/invented/private.pdf",
            "Label:synthetic.pdf:/invented/private.pdf",
            "synthetic.pdf:application//pdf",
            "synthetic.pdf:/pdf",
        ] {
            let (objects, probes, warnings) = check(text);
            assert_eq!(probes, 0, "ambiguous attachment must not be probed");
            assert!(
                !serde_json::to_string(&objects)
                    .expect("synthetic objects serialize")
                    .contains("invented/private")
            );
            assert!(!warnings.is_empty());
        }
    }

    #[test]
    fn attachment_expansion_is_bounded_before_metadata() {
        let (objects, probes, warnings) = check(&"x;".repeat(20_000));
        assert!(objects.len() <= 256);
        assert!(probes <= 256);
        assert!(warnings.iter().any(|warning| warning.code == WarningCode::WorkLimit));
    }
}
