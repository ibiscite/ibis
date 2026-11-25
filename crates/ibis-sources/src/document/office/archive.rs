//! A Word or OpenDocument file as the zip archive it is, read within fixed
//! bounds. Entries are checked before any is inflated, only the XML parts
//! Ibis reads are inflated, and every other entry is copied as stored bytes.
use super::{MAX_FILE_BYTES, Refusal};
use std::collections::BTreeSet;
use std::io::{Cursor, Read, Write};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAX_ENTRIES: usize = 10_000;
/// The most one XML part may inflate to.
const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;
/// The most all parts read may inflate to together.
const MAX_READ_BYTES: u64 = 256 * 1024 * 1024;

pub struct Archive<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    read: u64,
}

impl<'a> Archive<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, Refusal> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(Refusal::TooLarge);
        }
        if !bytes.starts_with(b"PK\x03\x04") {
            return Err(Refusal::Unsupported);
        }
        let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| Refusal::Damaged)?;
        if zip.len() > MAX_ENTRIES {
            return Err(Refusal::TooLarge);
        }
        let mut names = BTreeSet::new();
        for index in 0..zip.len() {
            let entry = zip.by_index_raw(index).map_err(|_| Refusal::Damaged)?;
            let name = entry.name().to_owned();
            // Names that could leave a folder, or two entries with one name,
            // never come from Word or LibreOffice; the copy would carry them.
            let plain = !name.is_empty()
                && !name.starts_with('/')
                && !name.contains(['\\', '\0', ':'])
                && name.split('/').all(|part| part != ".." && part != ".")
                && entry.enclosed_name().is_some();
            if !plain || !names.insert(name.to_lowercase()) {
                return Err(Refusal::Unsafe);
            }
            if entry.encrypted() {
                return Err(Refusal::Unsupported);
            }
            if !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            ) {
                return Err(Refusal::Damaged);
            }
        }
        Ok(Self { zip, read: 0 })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.zip.file_names()
    }

    pub fn has(&self, name: &str) -> bool {
        self.zip.index_for_name(name).is_some()
    }

    /// One part as UTF-8 text, inflated within the bounds, or `None` if absent.
    pub fn text(&mut self, name: &str) -> Result<Option<String>, Refusal> {
        let Some(index) = self.zip.index_for_name(name) else {
            return Ok(None);
        };
        let entry = self.zip.by_index(index).map_err(|_| Refusal::Damaged)?;
        // The declared size may lie; what inflates is what counts.
        let limit = MAX_PART_BYTES.min(MAX_READ_BYTES - self.read);
        let mut bytes = Vec::new();
        entry
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Refusal::Damaged)?;
        if bytes.len() as u64 > limit {
            return Err(Refusal::TooLarge);
        }
        self.read += bytes.len() as u64;
        String::from_utf8(bytes).map(Some).map_err(|_| Refusal::Damaged)
    }

    /// The same archive with `replaced` parts written anew and every other
    /// entry copied unchanged, in the original order.
    pub fn rewrite(&mut self, replaced: &[(&str, String)]) -> Result<Vec<u8>, Refusal> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..self.zip.len() {
            let entry = self.zip.by_index_raw(index).map_err(|_| Refusal::Damaged)?;
            match replaced.iter().find(|(name, _)| *name == entry.name()) {
                Some((name, text)) => {
                    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
                    drop(entry);
                    writer.start_file(*name, options).map_err(|_| Refusal::Damaged)?;
                    writer.write_all(text.as_bytes()).map_err(|_| Refusal::Damaged)?;
                }
                None => writer.raw_copy_file(entry).map_err(|_| Refusal::Damaged)?,
            }
        }
        Ok(writer.finish().map_err(|_| Refusal::Damaged)?.into_inner())
    }
}
