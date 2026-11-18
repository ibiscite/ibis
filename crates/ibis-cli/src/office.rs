//! The office child process, `ibis __office`: the protocol and the parent's
//! side are in `ibis_engine::office`. This side reads the zip archive and its
//! XML with pure-Rust parsers (`zip`, `quick-xml`) within fixed bounds.
pub use ibis_engine::office::*;
use std::io::{Read, Write};

mod archive;
mod docx;
mod odt;
mod xml;

/// The child process: bounded first, then [`child`].
pub fn run_child(arguments: &[std::ffi::OsString]) -> u8 {
    if !arguments.is_empty() {
        return 2;
    }
    #[cfg(unix)]
    if crate::pdf_text::limit_resources().is_err() {
        return 1;
    }
    child(std::io::stdin().lock(), std::io::stdout().lock())
}

/// Reads one framed request from `input` and answers on `output`: a JSON
/// line, then for `write` the new document's bytes.
pub fn child(mut input: impl Read, mut output: impl Write) -> u8 {
    let (answer, bytes) = match read_request(&mut input) {
        Err(refusal) => (Answer::Refused(refusal), Vec::new()),
        Ok((Command::Scan, document)) => match scan(&document) {
            Ok(scanned) => (Answer::Scanned(scanned), Vec::new()),
            Err(refusal) => (Answer::Refused(refusal), Vec::new()),
        },
        Ok((Command::Write(edits), document)) => match write(&document, &edits) {
            Ok(bytes) => (Answer::Written(bytes.len() as u64), bytes),
            Err(refusal) => (Answer::Refused(refusal), Vec::new()),
        },
    };
    let written = serde_json::to_writer(&mut output, &answer)
        .map_err(std::io::Error::from)
        .and_then(|()| output.write_all(b"\n"))
        .and_then(|()| output.write_all(&bytes))
        .and_then(|()| output.flush());
    match written {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

fn read_request(input: &mut impl Read) -> Result<(Command, Vec<u8>), Refusal> {
    let mut length = [0_u8; 8];
    input.read_exact(&mut length).map_err(|_| Refusal::Damaged)?;
    let length = u64::from_le_bytes(length);
    if length > MAX_COMMAND_BYTES {
        return Err(Refusal::TooLarge);
    }
    let mut json = Vec::new();
    input.take(length).read_to_end(&mut json).map_err(|_| Refusal::Damaged)?;
    let command = serde_json::from_slice(&json).map_err(|_| Refusal::Damaged)?;
    let mut document = Vec::new();
    input
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut document)
        .map_err(|_| Refusal::Damaged)?;
    if document.len() as u64 > MAX_FILE_BYTES {
        return Err(Refusal::TooLarge);
    }
    Ok((command, document))
}

/// Every paragraph of the document, the body's first and in order.
pub fn scan(bytes: &[u8]) -> Result<Scanned, Refusal> {
    let mut archive = archive::Archive::open(bytes)?;
    let kind = kind(&mut archive)?;
    let mut frames = xml::Frames::default();
    match kind {
        Kind::Docx => docx::scan(&mut archive, &mut frames)?,
        Kind::Odt => odt::scan(&mut archive, &mut frames)?,
    }
    Ok(Scanned {
        kind,
        paragraphs: frames.paragraphs,
    })
}

/// The document with `edits` applied, after checking each against its text.
pub fn write(bytes: &[u8], edits: &Edits) -> Result<Vec<u8>, Refusal> {
    let scanned = scan(bytes)?;
    let plan = xml::Plan::new(&scanned.paragraphs, edits)?;
    let mut archive = archive::Archive::open(bytes)?;
    match scanned.kind {
        Kind::Docx => docx::write(&mut archive, &plan),
        Kind::Odt => odt::write(&mut archive, &plan),
    }
}

fn kind(archive: &mut archive::Archive<'_>) -> Result<Kind, Refusal> {
    if let Some(mimetype) = archive.text("mimetype")? {
        return if mimetype.trim() == "application/vnd.oasis.opendocument.text" {
            Ok(Kind::Odt)
        } else {
            Err(Refusal::Unsupported)
        };
    }
    if !archive.has("word/document.xml") {
        return Err(Refusal::NotOffice);
    }
    let types = archive.text("[Content_Types].xml")?.ok_or(Refusal::NotOffice)?;
    // A macro-enabled document or a template would not open under .docx.
    if archive.has("word/vbaProject.bin")
        || !types.contains("application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml")
    {
        return Err(Refusal::Unsupported);
    }
    Ok(Kind::Docx)
}

#[cfg(test)]
#[path = "office_tests.rs"]
mod tests;
