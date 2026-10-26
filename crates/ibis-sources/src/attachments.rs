//! File-field decoding. Locations stay in the source owner, never the snapshot.
use crate::{SourceWarning, WarningCode};
use ibis_core::contract::{Attachment, Availability, ItemRef, LibraryObject};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MAX_RECORD_TOKENS: usize = 256;
const MAX_LIBRARY_ATTACHMENTS: usize = 100_000;
const MAX_LIBRARY_TOKENS: usize = 200_000;
const MAX_LIBRARY_BYTES: usize = 64 * 1024 * 1024;
const MAX_FIELD_BYTES: usize = 64 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_PATH_BYTES: usize = 4096;

#[derive(Default)]
struct AttachmentBudget {
    tokens: usize,
    attachments: usize,
    bytes: usize,
    limited: bool,
}
impl AttachmentBudget {
    fn warn(&mut self, warnings: &mut Vec<SourceWarning>, line: Option<usize>) {
        self.limited = true;
        warnings.push(SourceWarning {
            code: WarningCode::WorkLimit,
            line,
        });
    }
}

pub(crate) fn append(
    base: &Path,
    fields: Vec<(ItemRef, String, Option<usize>)>,
    objects: &mut Vec<LibraryObject>,
    paths: &mut BTreeMap<ItemRef, PathBuf>,
    warnings: &mut Vec<SourceWarning>,
) -> bool {
    append_with(
        base,
        fields,
        objects,
        paths,
        warnings,
        &mut AttachmentBudget::default(),
        availability,
    )
}

fn append_with(
    base: &Path,
    fields: Vec<(ItemRef, String, Option<usize>)>,
    objects: &mut Vec<LibraryObject>,
    paths: &mut BTreeMap<ItemRef, PathBuf>,
    warnings: &mut Vec<SourceWarning>,
    budget: &mut AttachmentBudget,
    mut probe: impl FnMut(&Path) -> Availability,
) -> bool {
    for (parent, field, line) in fields {
        if field.is_empty() {
            continue;
        }
        if field.len() > MAX_FIELD_BYTES || field.len() > MAX_LIBRARY_BYTES.saturating_sub(budget.bytes) {
            budget.warn(warnings, line);
            continue;
        }
        budget.bytes += field.len();
        let mut rest = Some(field.as_str());
        let mut index = 0;
        while rest.is_some() {
            if index >= MAX_RECORD_TOKENS
                || budget.tokens >= MAX_LIBRARY_TOKENS
                || budget.attachments >= MAX_LIBRARY_ATTACHMENTS
            {
                if rest.is_some_and(|rest| !rest.is_empty()) {
                    budget.warn(warnings, line);
                }
                break;
            }
            let encoded = match next_encoded(&mut rest, b';') {
                Ok(Some(encoded)) => encoded,
                Ok(None) => break,
                Err(()) => {
                    budget.warn(warnings, line);
                    break;
                }
            };
            let attachment_index = index;
            index += 1;
            budget.tokens += 1;
            if encoded.trim().is_empty() {
                continue;
            }
            let encoded = encoded.replace("\\;", ";");
            let id = ItemRef {
                library: parent.library.clone(),
                key: format!("attachment:{}:{attachment_index}", parent.key),
            };
            let parsed = decode(&encoded);
            if parsed
                .as_ref()
                .is_some_and(|(_, location, _)| location.len() > MAX_PATH_BYTES)
            {
                budget.warn(warnings, line);
                continue;
            }
            let (title, media_type, path, availability) = match parsed {
                Some((title, location, kind)) => match location_kind(&location) {
                    LocationKind::Online => (title, kind, None, Availability::Online),
                    LocationKind::Unsupported => {
                        warnings.push(SourceWarning {
                            code: WarningCode::InvalidAttachment,
                            line,
                        });
                        (title, kind, None, Availability::Unknown)
                    }
                    LocationKind::File => {
                        let relative = Path::new(&location);
                        let path = if relative.is_absolute() {
                            relative.to_path_buf()
                        } else {
                            base.join(relative)
                        };
                        if path.as_os_str().as_encoded_bytes().len() > MAX_PATH_BYTES {
                            budget.warn(warnings, line);
                            continue;
                        }
                        let availability = probe(&path);
                        (title, kind, Some(path), availability)
                    }
                },
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
            budget.attachments += 1;
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
    budget.limited
}

enum LocationKind {
    File,
    Online,
    Unsupported,
}
fn separator(byte: u8) -> bool {
    matches!(byte, b'/' | b'\\')
}
pub(crate) fn windows_drive(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
fn windows_namespace(text: &str) -> bool {
    let bytes = text.as_bytes();
    // Windows accepts either slash in a UNC/device prefix, including mixed
    // pairs. Single backslash roots and NT device namespaces are unsupported.
    if bytes.first() == Some(&b'\\') || bytes.len() >= 2 && separator(bytes[0]) && separator(bytes[1]) {
        return true;
    }
    bytes.first().is_some_and(|byte| separator(*byte))
        && text
            .trim_start_matches(['/', '\\'])
            .split(['/', '\\'])
            .next()
            .is_some_and(|part| {
                ["??", "device", "global??", "globalroot"]
                    .iter()
                    .any(|name| part.eq_ignore_ascii_case(name))
            })
}
fn scheme(text: &str) -> Option<(&str, &str)> {
    let (name, rest) = text.split_once(':')?;
    (name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte)))
    .then_some((name, rest))
}
fn direct_uri(text: &str) -> bool {
    scheme(text).is_some_and(|(name, rest)| {
        rest.starts_with("//")
            || [
                "file", "http", "https", "ftp", "sftp", "smb", "nfs", "ssh", "dav", "davs", "data", "mailto", "urn",
                "about",
            ]
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known))
    })
}
fn location_kind(text: &str) -> LocationKind {
    if windows_namespace(text) {
        return LocationKind::Unsupported;
    }
    if windows_drive(text) {
        return if cfg!(windows) && text.as_bytes().get(2).is_some_and(|byte| separator(*byte)) {
            LocationKind::File
        } else {
            LocationKind::Unsupported
        };
    }
    if let Some((name, rest)) = scheme(text) {
        return if (name.eq_ignore_ascii_case("http") || name.eq_ignore_ascii_case("https")) && rest.starts_with("//") {
            LocationKind::Online
        } else {
            LocationKind::Unsupported
        };
    }
    LocationKind::File
}
fn decode(text: &str) -> Option<(String, String, String)> {
    let text = text.trim();
    if text.len() > MAX_TOKEN_BYTES || text.chars().any(char::is_control) {
        return None;
    }
    // Recognise URI syntax before colons can be mistaken for MIME separators.
    if direct_uri(text) {
        return matches!(location_kind(text), LocationKind::Online)
            .then(|| (String::new(), text.into(), String::new()));
    }
    if windows_drive(text) {
        return (!text[2..].contains(':')).then(|| (String::new(), text.into(), mime(text)));
    }
    let fields = split_escaped(text, ':')?;
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
    if text.len() > 255 {
        return false;
    }
    if ["pdf", "html", "epub", "txt"]
        .iter()
        .any(|kind| text.eq_ignore_ascii_case(kind))
    {
        return true;
    }
    let Some((kind, subtype)) = text.split_once('/') else {
        return false;
    };
    fn token(text: &str) -> bool {
        (1..=127).contains(&text.len())
            && text.as_bytes()[0].is_ascii_alphanumeric()
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$&-^_.+".contains(&byte))
    }
    token(kind) && token(subtype)
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
// Scan lazily with a byte bound; never allocate all semicolon-separated tokens.
// ASCII delimiter positions are UTF-8 boundaries, even though this scans bytes.
fn next_encoded<'a>(rest: &mut Option<&'a str>, delimiter: u8) -> Result<Option<&'a str>, ()> {
    let Some(text) = rest.take() else {
        return Ok(None);
    };
    let bytes = text.as_bytes();
    let mut position = 0;
    while position < bytes.len() {
        if position > MAX_TOKEN_BYTES {
            return Err(());
        }
        if bytes[position] == b'\\' && bytes.get(position + 1) == Some(&delimiter) {
            position += 2;
        } else if bytes[position] == delimiter {
            *rest = Some(&text[position + 1..]);
            return Ok(Some(&text[..position]));
        } else {
            position += 1;
        }
    }
    if text.len() > MAX_TOKEN_BYTES {
        return Err(());
    }
    Ok(Some(text))
}
// Only delimiter escapes are decoded; ordinary Windows backslashes stay literal.
fn split_escaped(text: &str, delimiter: char) -> Option<Vec<String>> {
    let mut rest = Some(text);
    let mut values = Vec::with_capacity(3);
    while let Some(value) = next_encoded(&mut rest, delimiter as u8).ok()? {
        if values.len() == 3 {
            return None;
        }
        values.push(value.replace(
            if delimiter == ':' { "\\:" } else { "\\;" },
            if delimiter == ':' { ":" } else { ";" },
        ));
    }
    Some(values)
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
            Some(vec!["one;two.pdf".into(), "three.pdf".into()])
        );
    }
}

#[cfg(test)]
mod adversarial_tests {
    use super::*;
    use ibis_core::contract::LibraryId;

    pub(super) fn check(text: &str) -> (Vec<LibraryObject>, usize, Vec<SourceWarning>) {
        check_with_budget(text, &mut AttachmentBudget::default())
    }
    pub(super) fn check_with_budget(
        text: &str,
        budget: &mut AttachmentBudget,
    ) -> (Vec<LibraryObject>, usize, Vec<SourceWarning>) {
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
            budget,
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
    fn paths_cannot_become_media_types() -> Result<(), serde_json::Error> {
        for text in [
            "file:///invented/private.pdf",
            "synthetic.pdf:/invented/private.pdf",
            "Label:synthetic.pdf:/invented/private.pdf",
            "synthetic.pdf:application//pdf",
            "synthetic.pdf:/pdf",
        ] {
            let (objects, probes, warnings) = check(text);
            assert_eq!(probes, 0, "ambiguous attachment must not be probed");
            assert!(!serde_json::to_string(&objects)?.contains("invented/private"));
            assert!(!warnings.is_empty());
        }
        Ok(())
    }

    #[test]
    fn attachment_expansion_is_bounded_before_metadata() {
        let (objects, probes, warnings) = check(&"x;".repeat(20_000));
        assert!(objects.len() <= 256);
        assert!(probes <= 256);
        assert!(warnings.iter().any(|warning| warning.code == WarningCode::WorkLimit));
    }
}

#[cfg(test)]
mod limit_tests {
    use super::adversarial_tests::{check, check_with_budget};
    use super::*;

    #[test]
    fn library_count_work_and_byte_limits_stop_before_probing() {
        for mut budget in [
            AttachmentBudget {
                attachments: MAX_LIBRARY_ATTACHMENTS - 1,
                ..AttachmentBudget::default()
            },
            AttachmentBudget {
                tokens: MAX_LIBRARY_TOKENS - 1,
                ..AttachmentBudget::default()
            },
            AttachmentBudget {
                bytes: MAX_LIBRARY_BYTES - 1,
                ..AttachmentBudget::default()
            },
        ] {
            let (objects, probes, warnings) = check_with_budget("x.pdf;y.pdf", &mut budget);
            assert!(objects.len() <= 1);
            assert!(probes <= 1);
            assert!(warnings.iter().any(|warning| warning.code == WarningCode::WorkLimit));
        }
        // A library with exactly 100k ordinary single-attachment references fits.
        let mut last_slot = AttachmentBudget {
            attachments: MAX_LIBRARY_ATTACHMENTS - 1,
            tokens: MAX_LIBRARY_ATTACHMENTS - 1,
            ..AttachmentBudget::default()
        };
        let (objects, probes, warnings) = check_with_budget("last.pdf", &mut last_slot);
        assert_eq!((objects.len(), probes, warnings.len()), (1, 1, 0));
    }

    #[test]
    fn oversized_paths_tokens_and_fields_never_reach_metadata() {
        for text in [
            "x".repeat(MAX_PATH_BYTES + 1),
            "x".repeat(MAX_TOKEN_BYTES + 1),
            ";".repeat(MAX_FIELD_BYTES + 1),
        ] {
            let (objects, probes, warnings) = check(&text);
            assert!(objects.is_empty());
            assert_eq!(probes, 0);
            assert!(warnings.iter().any(|warning| warning.code == WarningCode::WorkLimit));
        }
    }

    #[test]
    fn valid_mime_tokens_and_naked_drive_paths_keep_their_meaning() {
        for kind in ["PDF", "application/pdf", "application/vnd.synthetic+json", "TEXT/PLAIN"] {
            assert!(is_media_type(kind));
        }
        for kind in [
            "/private.pdf",
            "application/",
            "/pdf",
            "application//pdf",
            "application/pdf/path",
            "application/pdf;name=x",
        ] {
            assert!(!is_media_type(kind));
        }
        assert!(!is_media_type(&format!("application/{}", "x".repeat(128))));
        for location in [r"C:\invented\private.pdf", "C:/invented/private.pdf"] {
            let (objects, probes, _) = check(location);
            assert_eq!(probes, usize::from(cfg!(windows)));
            let LibraryObject::Attachment(attachment) = &objects[0] else {
                panic!("synthetic attachment")
            };
            assert_eq!(attachment.media_type, "application/pdf");
        }
    }
}

#[cfg(test)]
mod identity_budget_tests {
    use super::*;
    use ibis_core::contract::LibraryId;

    #[test]
    fn long_parent_keys_cannot_amplify_retained_attachment_identity_bytes() {
        for (key_length, location) in [(64 * 1024, "https://example.invalid/x"), (1024, "x.pdf")] {
            let parent = ItemRef {
                library: LibraryId::file(b"invented"),
                key: format!("record:{}", "x".repeat(key_length)),
            };
            let mut objects = Vec::new();
            let mut paths = BTreeMap::new();
            let mut warnings = Vec::new();
            let mut probes = 0;
            append_with(
                Path::new("/invented"),
                vec![(parent, format!("{location};").repeat(256), Some(3))],
                &mut objects,
                &mut paths,
                &mut warnings,
                &mut AttachmentBudget::default(),
                |_| {
                    probes += 1;
                    Availability::BrokenLink
                },
            );
            let retained = objects
                .iter()
                .map(|object| match object {
                    LibraryObject::Attachment(item) => {
                        item.id.key.len()
                            + item.id.library.as_str().len()
                            + item
                                .parent
                                .as_ref()
                                .map_or(0, |parent| parent.key.len() + parent.library.as_str().len())
                    }
                    _ => 0,
                })
                .sum::<usize>()
                + paths
                    .keys()
                    .map(|id| id.key.len() + id.library.as_str().len())
                    .sum::<usize>();
            assert!(retained <= 256 * 1024, "retained {retained} identity bytes");
            assert!(objects.len() < 256);
            assert!(
                warnings
                    .iter()
                    .any(|warning| warning.code == WarningCode::WorkLimit && warning.line == Some(3))
            );
            if location.starts_with("https:") {
                assert_eq!(probes, 0);
            }
        }
    }
}

#[cfg(test)]
mod identity_library_budget_test {
    use super::*;
    use ibis_core::contract::LibraryId;
    #[test]
    fn exhausted_reference_budget_refuses_identity_clones_and_probes() {
        let parent = ItemRef {
            library: LibraryId::file(b"invented"),
            key: format!("record:{}", "x".repeat(1024)),
        };
        let mut objects = Vec::new();
        let mut paths = BTreeMap::new();
        let mut warnings = Vec::new();
        let mut budget = AttachmentBudget {
            bytes: MAX_LIBRARY_BYTES - 128,
            ..AttachmentBudget::default()
        };
        let mut probes = 0;
        append_with(
            Path::new("/invented"),
            vec![(parent, "x.pdf".into(), Some(2))],
            &mut objects,
            &mut paths,
            &mut warnings,
            &mut budget,
            |_| {
                probes += 1;
                Availability::BrokenLink
            },
        );
        assert!(objects.is_empty());
        assert!(paths.is_empty());
        assert_eq!(probes, 0);
        assert!(warnings.iter().any(|warning| warning.code == WarningCode::WorkLimit));
    }
}
