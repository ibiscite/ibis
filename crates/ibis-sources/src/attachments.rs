//! File-field decoding. Locations stay in the source owner, never the snapshot.
use crate::{SourceWarning, WarningCode};
use ibis_core::contract::{Attachment, Availability, ItemRef, LibraryObject};
use std::path::Path;

const MAX_RECORD_TOKENS: usize = 256;
const MAX_LIBRARY_ATTACHMENTS: usize = 100_000;
const MAX_LIBRARY_TOKENS: usize = 200_000;
const MAX_LIBRARY_BYTES: usize = 64 * 1024 * 1024;
const MAX_FIELD_BYTES: usize = 64 * 1024;
const MAX_RECORD_WORK_BYTES: usize = 256 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
pub(crate) const MAX_PATH_BYTES: usize = 4096;

#[derive(Default)]
struct AttachmentBudget {
    tokens: usize,
    attachments: usize,
    bytes: usize,
}
impl AttachmentBudget {
    fn charge(&mut self, record_work: &mut usize, bytes: usize) -> bool {
        if bytes > MAX_RECORD_WORK_BYTES.saturating_sub(*record_work)
            || bytes > MAX_LIBRARY_BYTES.saturating_sub(self.bytes)
        {
            return false;
        }
        *record_work += bytes;
        self.bytes += bytes;
        true
    }
}

pub(crate) fn append(
    fields: Vec<(ItemRef, String, Option<usize>)>,
    objects: &mut Vec<LibraryObject>,
    warnings: &mut Vec<SourceWarning>,
) -> Vec<(usize, String)> {
    let budget = &mut AttachmentBudget::default();
    let mut references = Vec::new();
    for (parent, field, line) in fields {
        if field.is_empty() {
            continue;
        }
        if field.len() > MAX_FIELD_BYTES || field.len() > MAX_LIBRARY_BYTES.saturating_sub(budget.bytes) {
            warnings.push(SourceWarning::at(WarningCode::WorkLimit, line));
            continue;
        }
        budget.bytes += field.len();
        let mut record_work = field.len();
        let mut rest = Some(field.as_str());
        let mut index = 0;
        while rest.is_some() {
            if index >= MAX_RECORD_TOKENS
                || budget.tokens >= MAX_LIBRARY_TOKENS
                || budget.attachments >= MAX_LIBRARY_ATTACHMENTS
            {
                if rest.is_some_and(|rest| !rest.is_empty()) {
                    warnings.push(SourceWarning::at(WarningCode::WorkLimit, line));
                }
                break;
            }
            let encoded = match next_encoded(&mut rest, b';') {
                Ok(Some(encoded)) => encoded,
                Ok(None) => break,
                Err(()) => {
                    warnings.push(SourceWarning::at(WarningCode::WorkLimit, line));
                    break;
                }
            };
            let attachment_index = index;
            index += 1;
            budget.tokens += 1;
            if encoded.trim().is_empty() {
                continue;
            }
            let digits = attachment_index
                .checked_ilog10()
                .map_or(1, |digits| digits as usize + 1);
            let id_bytes = parent.key.len().saturating_add("attachment:".len() + 1 + digits);
            // Reserve the ID and parent before identity cloning or formatting.
            let reference_work = id_bytes
                .saturating_add(parent.key.len())
                .saturating_add(parent.library.as_str().len().saturating_mul(2))
                .saturating_add(digits);
            if !budget.charge(&mut record_work, reference_work) {
                warnings.push(SourceWarning::at(WarningCode::WorkLimit, line));
                break;
            }
            let encoded = encoded.replace("\\;", ";");
            let parsed = decode(&encoded);
            if parsed
                .as_ref()
                .is_some_and(|(_, location, _)| location.len() > MAX_PATH_BYTES)
            {
                warnings.push(SourceWarning::at(WarningCode::WorkLimit, line));
                continue;
            }
            let (title, media_type, path, availability) = match parsed {
                Some((title, location, kind)) => match location_kind(&location) {
                    LocationKind::Online => (title, kind, None, Availability::Online),
                    LocationKind::Unsupported => {
                        warnings.push(SourceWarning::at(WarningCode::InvalidAttachment, line));
                        (title, kind, None, Availability::Unknown)
                    }
                    LocationKind::File => (title, kind, Some(location), Availability::Unknown),
                },
                None => {
                    warnings.push(SourceWarning::at(WarningCode::InvalidAttachment, line));
                    (String::new(), String::new(), None, Availability::Unknown)
                }
            };
            // Exact reservation avoids geometric growth while appending a long
            // native key followed by its numeric attachment suffix.
            let mut key = String::with_capacity(id_bytes);
            key.push_str("attachment:");
            key.push_str(&parent.key);
            key.push(':');
            key.push_str(&attachment_index.to_string());
            let id = ItemRef {
                library: parent.library.clone(),
                key,
            };
            if let Some(path) = path {
                references.push((objects.len(), path));
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
    references
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
pub(crate) fn scheme(text: &str) -> Option<(&str, &str)> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::contract::LibraryId;
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

    fn check(text: &str) -> (Vec<LibraryObject>, usize, Vec<SourceWarning>) {
        let parent = ItemRef {
            library: LibraryId::digest(b"invented"),
            key: "record:one".into(),
        };
        let mut objects = Vec::new();
        let mut warnings = Vec::new();
        let paths = append(vec![(parent, text.into(), Some(2))], &mut objects, &mut warnings);
        (objects, paths.len(), warnings)
    }

    #[test]
    fn remote_windows_prefixes_are_not_local_references() {
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
            let (_, references, warnings) = check(location);
            assert_eq!(references, 0, "unsupported location must not be probed: {location}");
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
            let (objects, references, warnings) = check(text);
            assert_eq!(references, 0, "ambiguous attachment must not be probed");
            assert!(!serde_json::to_string(&objects)?.contains("invented/private"));
            assert!(!warnings.is_empty());
        }
        Ok(())
    }

    #[test]
    fn attachment_expansion_is_bounded_before_metadata() {
        let (objects, references, warnings) = check(&"x;".repeat(20_000));
        assert!(objects.len() <= 256);
        assert!(references <= 256);
        assert!(warnings.iter().any(|warning| warning.code == WarningCode::WorkLimit));
    }

    #[test]
    fn oversized_paths_tokens_and_fields_warn_without_retaining_references() {
        for text in [
            "x".repeat(MAX_PATH_BYTES + 1),
            "x".repeat(MAX_TOKEN_BYTES + 1),
            ";".repeat(MAX_FIELD_BYTES + 1),
        ] {
            let (objects, references, warnings) = check(&text);
            assert!(objects.is_empty());
            assert_eq!(references, 0);
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
            let (objects, references, _) = check(location);
            assert_eq!(references, usize::from(cfg!(windows)));
            let LibraryObject::Attachment(attachment) = &objects[0] else {
                panic!("synthetic attachment")
            };
            assert_eq!(attachment.media_type, "application/pdf");
        }
    }
}
