//! Your styles: `.csl` files you kept or added, one `<id>.csl` each in one
//! folder that Ibis, Ibis Nightly and `ibis` share (decision 0007). A style there
//! wins over the bundled style of the same id. Every file is outside input: it
//! is read when used, from a regular file under its exact name, at most 1 MiB.
use crate::FormatError;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub(crate) const MAX_BYTES: usize = 1024 * 1024;

static FOLDER: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Where your styles are kept; `None` means you have none. Front ends name
/// it once at start, so a test can give each case its own folder.
pub fn use_styles_folder(folder: Option<PathBuf>) {
    if let Ok(mut current) = FOLDER.write() {
        *current = folder;
    }
}

fn folder() -> Option<PathBuf> {
    FOLDER.read().ok().and_then(|folder| folder.clone())
}

/// An id a file of yours can carry: lower case letters, digits and hyphens, as
/// the official styles' ids are.
pub(crate) fn valid_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !id.starts_with('-')
}

/// Your style `id`, if you have one. A file that is there but cannot be read
/// as text is `InvalidStyle`: it is yours, so nothing else answers for it.
pub(crate) fn read(id: &str) -> Result<Option<String>, FormatError> {
    let Some(folder) = folder() else { return Ok(None) };
    if !valid_id(id) {
        return Ok(None);
    }
    // Only the name the folder lists: on a disk that ignores letter case,
    // `APA.csl` would otherwise open as `apa.csl`.
    let name = format!("{id}.csl");
    if !listed(&folder)?.contains(&name) {
        return Ok(None);
    }
    read_file(&folder.join(name)).map(Some)
}

/// Every id you have a file for, sorted; names that are not ids are ignored.
pub(crate) fn ids() -> Result<Vec<String>, FormatError> {
    let Some(folder) = folder() else { return Ok(vec![]) };
    let mut ids: Vec<String> = listed(&folder)?
        .into_iter()
        .filter_map(|name| name.strip_suffix(".csl").filter(|id| valid_id(id)).map(str::to_owned))
        .collect();
    ids.sort();
    Ok(ids)
}

fn listed(folder: &Path) -> Result<Vec<String>, FormatError> {
    match fs::read_dir(folder) {
        Ok(entries) => Ok(entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(_) => Err(FormatError::InvalidStyle),
    }
}

fn read_file(path: &Path) -> Result<String, FormatError> {
    // Not a link: it could point anywhere. The open file is checked again, so
    // a swap between the two checks still reads only a regular file.
    let link = fs::symlink_metadata(path).map_err(|_| FormatError::InvalidStyle)?;
    if !link.file_type().is_file() {
        return Err(FormatError::InvalidStyle);
    }
    let file = fs::File::open(path).map_err(|_| FormatError::InvalidStyle)?;
    if !file.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Err(FormatError::InvalidStyle);
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FormatError::InvalidStyle)?;
    if bytes.len() > MAX_BYTES {
        return Err(FormatError::InvalidStyle);
    }
    String::from_utf8(bytes).map_err(|_| FormatError::InvalidStyle)
}

/// Write your style `id` whole: a new file in the same folder, then a rename,
/// so a reader sees the old file or the new one, never part of one.
pub(crate) fn write(id: &str, xml: &str) -> Result<(), FormatError> {
    let folder = folder().ok_or(FormatError::StylesFolder)?;
    if !valid_id(id) || xml.len() > MAX_BYTES {
        return Err(FormatError::InvalidStyle);
    }
    fs::create_dir_all(&folder).map_err(|_| FormatError::StylesFolder)?;
    let temporary = folder.join(format!(".{id}.{}.tmp", std::process::id()));
    let written = (|| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(xml.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, folder.join(format!("{id}.csl")))
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written.map_err(|_| FormatError::StylesFolder)
}

/// Remove your style `id`. `false` when you have none.
pub(crate) fn remove(id: &str) -> Result<bool, FormatError> {
    let folder = folder().ok_or(FormatError::StylesFolder)?;
    if !ids()?.iter().any(|listed| listed == id) {
        return Ok(false);
    }
    fs::remove_file(folder.join(format!("{id}.csl")))
        .map(|()| true)
        .map_err(|_| FormatError::StylesFolder)
}
