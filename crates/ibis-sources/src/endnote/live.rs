//! An open EndNote library, read through EndNote's scripting interface
//! (`EndNote.sdef`): `retrieve "all"` lists the records and `export … as
//! "RSXML"` gives the same XML as File › Export. EndNote is never started,
//! and the library is only read.
//!
//! macOS asks the person once whether the program that runs Ibis (Terminal,
//! an editor or the Ibis app) may control EndNote; without that, the read
//! is refused with `permission-denied`.
use std::process::Command;
use std::time::{Duration, Instant};

use crate::child::{self, ChildError, Limits};

use ibis_core::contract::Fingerprint;
use ibis_core::contract::adapter::{AdapterRead, AdapterRefusal};

use super::xml::{self, XmlRecord};

const SEPARATOR: char = '\u{1f}';
const CHUNK: char = '\u{1e}';
const MAX_DIAGNOSTICS: u64 = 16 * 1024;

/// Lists the chosen library and exports its records 200 at a time.
const EXPORT: &str = r#"use framework "Foundation"
use scripting additions

on run argv
  set wanted to item 1 of argv
  set maxRecords to item 2 of argv as integer
  set leftBytes to item 3 of argv as integer
  set stream to current application's NSFileHandle's fileHandleWithStandardOutput()
  if not (application id "com.ThomsonResearchSoft.EndNote" is running) then return "closed"
  tell application id "com.ThomsonResearchSoft.EndNote"
    with timeout of 100 seconds
      set chosen to missing value
      if wanted is "" then
        if (count of documents) is 0 then return "missing"
        if (count of documents) > 1 then return "needed"
        set chosen to document 1
      else
        repeat with d in documents
          set p to ""
          try
            set p to POSIX path of (file of d as alias)
          end try
          considering case
            if (name of d) is wanted or p is wanted then
              if chosen is not missing value then return "needed"
              set chosen to contents of d
            end if
          end considering
        end repeat
        if chosen is missing value then return "missing"
      end if
      set p to POSIX path of (file of chosen as alias)
      set ids to retrieve "all" records in chosen
      set n to count of ids
      if n > maxRecords then error "Record limit"
      set leftBytes to my emitText(stream, "ok" & (character id 31) & p & (character id 31) & n, leftBytes)
      set i to 1
      repeat while i is less than or equal to n
        set j to i + 199
        if j > n then set j to n
        set part to export (items i thru j of ids) as "RSXML"
        set leftBytes to my emitText(stream, (character id 30) & part, leftBytes)
        set part to missing value
        set i to j + 1
      end repeat
      return ""
    end timeout
  end tell
end run

on emitText(stream, part, leftBytes)
  set textValue to current application's NSString's stringWithString:part
  set utf8 to current application's NSUTF8StringEncoding
  set sizeBytes to (textValue's lengthOfBytesUsingEncoding:utf8) as integer
  if sizeBytes > leftBytes then error "Export limit"
  stream's writeData:(textValue's dataUsingEncoding:utf8)
  return leftBytes - sizeBytes
end emitText
"#;

/// One field of one record, as EndNote shows it.
const FIELD: &str = r#"on run argv
  set wantedPath to item 1 of argv
  set wantedNumber to item 2 of argv
  set fieldName to item 3 of argv
  set maxRecords to item 4 of argv as integer
  tell application id "com.ThomsonResearchSoft.EndNote"
    with timeout of 30 seconds
      repeat with d in documents
        set p to ""
        try
          set p to POSIX path of (file of d as alias)
        end try
        considering case
          if p is wantedPath then
            set ids to retrieve "all" records in d
            if (count of ids) > maxRecords then error "Record limit"
            repeat with r in ids
              if (r as text) contains ("recNum=\"" & wantedNumber & "\"") then
                set v to field fieldName of record (contents of r)
                if v is missing value then set v to ""
                return "ok" & (character id 31) & v
              end if
            end repeat
          end if
        end considering
      end repeat
    end timeout
  end tell
  return "missing"
end run
"#;

pub(crate) fn read(library: Option<&str>) -> Result<AdapterRead, AdapterRefusal> {
    let deadline = Instant::now() + crate::adapter::DEADLINE;
    let answer = osascript(
        EXPORT,
        &[
            library.unwrap_or(""),
            &crate::MAX_LIBRARY_RECORDS.to_string(),
            // osascript adds a newline to the script's result.
            &(super::MAX_FILE_BYTES - 1).to_string(),
        ],
        deadline,
        super::MAX_FILE_BYTES,
    )?;
    let mut parts = answer.split(CHUNK);
    let header: Vec<&str> = parts.next().unwrap_or_default().split(SEPARATOR).collect();
    let (path, count) = match header[..] {
        ["closed"] => return Err(AdapterRefusal::ManagerClosed),
        ["needed"] => return Err(AdapterRefusal::LibraryNeeded),
        ["missing"] => return Err(AdapterRefusal::LibraryNotFound),
        ["ok", path, count] => (
            path.to_owned(),
            count.parse::<usize>().map_err(|_| AdapterRefusal::Unreadable)?,
        ),
        _ => return Err(AdapterRefusal::Unreadable),
    };
    let chunks: Vec<&str> = parts.collect();
    if count > crate::MAX_LIBRARY_RECORDS {
        return Err(AdapterRefusal::Unreadable);
    }
    let records = decoded(&chunks, &path, count, deadline)?;
    // Every record EndNote listed, or none: a short export is no library.
    if records.len() != count {
        return Err(AdapterRefusal::Unreadable);
    }
    let db_id = records
        .iter()
        .find_map(|record| record.db_id.clone())
        .unwrap_or_default();
    let identity = file_identity(&path)?;
    let scope = Fingerprint::of_bytes(
        &[
            b"live\0",
            db_id.as_bytes(),
            b"\0",
            path.as_bytes(),
            b"\0",
            identity.as_bytes(),
        ]
        .concat(),
    );
    let mut skipped = Vec::new();
    let placed: Vec<(Option<String>, &XmlRecord)> = records
        .iter()
        .filter_map(|record| match (record.one("rec-number"), &record.db_id) {
            (Some(number), Some(id)) if *id == db_id && number.bytes().all(|b| b.is_ascii_digit()) => {
                Some((Some(number.to_owned()), record))
            }
            _ => {
                skipped.push(super::unplaced(record));
                None
            }
        })
        .collect();
    let mut read = super::read(scope, placed.into_iter());
    read.skipped.extend(skipped);
    Ok(read)
}

/// The export's records, decoded as EndNote meant them. When the plain and
/// the Mac Roman repaired texts would map differently, one field EndNote
/// reads directly decides which is right; if neither matches, the read is
/// refused rather than guessed.
fn decoded(chunks: &[&str], path: &str, count: usize, deadline: Instant) -> Result<Vec<XmlRecord>, AdapterRefusal> {
    let plain = collect_records(chunks.iter().copied().map(Some), count);
    let repaired = collect_records(chunks.iter().map(|text| super::macroman::repair(text)), count);
    let (plain, repaired) = match (plain, repaired) {
        (Some(plain), Some(repaired)) => (plain, repaired),
        (Some(only), None) | (None, Some(only)) => return Ok(only),
        (None, None) => return Err(AdapterRefusal::Unreadable),
    };
    if plain.len() != repaired.len() {
        return Err(AdapterRefusal::Unreadable);
    }
    let mut undecided = false;
    let mut differing = None;
    'records: for (a, b) in plain.iter().zip(&repaired) {
        let kind = a.one("ref-type").unwrap_or_default();
        for (tag, labels) in CHECKED {
            if a.fields.get(*tag) != b.fields.get(*tag) {
                match labels(kind) {
                    Some(label) => {
                        differing = Some((a, b, *tag, label));
                        break 'records;
                    }
                    None => undecided = true,
                }
            }
        }
    }
    let Some((a, b, tag, label)) = differing else {
        // Unless a mapped field differs, either reading gives the same records.
        return if undecided {
            Err(AdapterRefusal::Unreadable)
        } else {
            Ok(repaired)
        };
    };
    let number = a.one("rec-number").ok_or(AdapterRefusal::Unreadable)?;
    let answer = osascript(
        FIELD,
        &[path, number, label, &crate::MAX_LIBRARY_RECORDS.to_string()],
        deadline,
        super::MAX_FILE_BYTES,
    )?;
    let shown = answer
        .strip_prefix("ok\u{1f}")
        .ok_or(AdapterRefusal::Unreadable)?
        .trim()
        .to_owned();
    let joined = |record: &XmlRecord| {
        record
            .fields
            .get(tag)
            .map(|values| values.join("\r"))
            .unwrap_or_default()
    };
    match (joined(a) == shown, joined(b) == shown) {
        (true, false) => Ok(plain),
        (false, true) => Ok(repaired),
        _ => Err(AdapterRefusal::Unreadable),
    }
}

/// Parse one XML chunk at a time and check the running record count.
fn collect_records(texts: impl Iterator<Item = Option<impl AsRef<str>>>, count: usize) -> Option<Vec<XmlRecord>> {
    let mut records = Vec::new();
    for text in texts {
        let limit = count - records.len();
        records.extend(xml::records_up_to(text?.as_ref(), limit).ok()?);
    }
    Some(records)
}

type Labels = fn(&str) -> Option<&'static str>;

/// The mapped tags, each with EndNote's label for it by reference type.
const CHECKED: &[(&str, Labels)] = &[
    ("titles/title", |_| Some("Title")),
    ("contributors/authors/author", |_| Some("Author")),
    ("titles/secondary-title", |kind| match kind {
        "17" => Some("Journal"),
        "6" => Some("Series Title"),
        "5" => Some("Book Title"),
        _ => None,
    }),
    ("contributors/secondary-authors/author", |kind| match kind {
        "6" => Some("Series Editor"),
        "5" => Some("Editor"),
        _ => None,
    }),
    ("contributors/tertiary-authors/author", |kind| {
        (kind == "6").then_some("Editor")
    }),
    ("publisher", |_| Some("Publisher")),
    ("pub-location", |_| Some("Place Published")),
    ("periodical/full-title", |_| None),
    ("volume", |_| Some("Volume")),
    ("number", |kind| (kind == "17").then_some("Issue")),
    ("pages", |kind| {
        Some(if kind == "6" { "Number of Pages" } else { "Pages" })
    }),
    ("edition", |_| Some("Edition")),
    ("electronic-resource-num", |_| Some("DOI")),
    ("dates/year", |_| Some("Year")),
    ("ref-type", |_| None),
    ("rec-number", |_| None),
];

/// The device, inode and creation time of the library's `.Data` folder,
/// which holds its records. EndNote rewrites the `.enl` file itself when a
/// library edited in place closes, but keeps the folder; a copied, restored
/// or replaced library has another one.
#[allow(
    clippy::disallowed_methods,
    reason = "Metadata of the library EndNote has open; nothing is read from it."
)]
fn file_identity(path: &str) -> Result<String, AdapterRefusal> {
    use std::os::unix::fs::MetadataExt;
    let data = std::path::Path::new(path).with_extension("Data");
    let metadata = std::fs::metadata(data).map_err(|_| AdapterRefusal::Unreadable)?;
    if !metadata.is_dir() {
        return Err(AdapterRefusal::Unreadable);
    }
    let created = metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos());
    Ok(format!("{}:{}:{created}", metadata.dev(), metadata.ino()))
}

/// Run an AppleScript with `arguments`; its answer without the final newline.
fn osascript(script: &str, arguments: &[&str], deadline: Instant, max_stdout: u64) -> Result<String, AdapterRefusal> {
    let mut command = Command::new("/usr/bin/osascript");
    command.arg("-").args(arguments);
    let timeout = if script == FIELD { 30 } else { 110 };
    let time = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(timeout));
    let output = child::run_helper(
        command,
        script.as_bytes(),
        Limits {
            time,
            stdout: max_stdout,
            stderr: Some(MAX_DIAGNOSTICS),
        },
    )
    .map_err(|error| match error {
        ChildError::Unavailable => AdapterRefusal::Unsupported,
        _ => AdapterRefusal::Unreadable,
    })?;
    if !output.status.success() {
        let errors = String::from_utf8_lossy(&output.stderr);
        // Only fixed error numbers are looked at; EndNote's message is never kept.
        return Err(if errors.contains("(-1743)") {
            AdapterRefusal::PermissionDenied
        } else if errors.contains("(-600)") || errors.contains("(-609)") {
            AdapterRefusal::ManagerClosed
        } else {
            AdapterRefusal::Unreadable
        });
    }
    let mut text = String::from_utf8(output.stdout).map_err(|_| AdapterRefusal::Unreadable)?;
    if text.ends_with('\n') {
        text.pop();
    }
    Ok(text)
}
