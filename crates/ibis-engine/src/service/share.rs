//! A pack of references to hand to someone (#1953, decision 0051): a reading
//! list page in the person's style, a `.bib` file and, only when asked, the
//! references' PDFs, zipped by default.
//!
//! Every file is made from share-safe fields: the reading list by the CSL
//! formatter and the `.bib` by the BibTeX export, both through
//! `CitableFields`, so notes, annotations, private links and device paths
//! never enter the pack. PDFs come only from inside their library's folder or
//! Zotero's storage, opened under the PDF reader's rules (a plain local file,
//! never a link), whatever this computer allows AI apps to read. One plan
//! serves the preview and the write, so both name the same files and sizes.
//! Nothing that exists is replaced, and no location appears in the answer.
use super::reading::Owner;
use super::*;
use crate::request::ShareRequest;
use ibis_core::citable::CitableFields;
use ibis_sources::pdf::LocalPdf;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// The reading list's place in the pack.
pub const READING_LIST: &str = "Reading list.html";
/// The BibTeX file's place in the pack.
pub const BIBTEX_FILE: &str = "References.bib";
/// The folder the PDFs go in.
pub const PDF_FOLDER: &str = "PDFs";
/// The most bytes one pack holds, well within what any zip reader opens
/// without the 64-bit extensions.
pub const MAX_PACK_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// The most files one pack holds.
pub const MAX_PACK_FILES: usize = 10_000;
/// A PDF's name before `.pdf`, in bytes: short enough for every system.
const MAX_NAME_BYTES: usize = 120;

/// What a pack holds, or would hold.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShareResult {
    /// The pack was written; false when no destination was given.
    pub written: bool,
    /// Each file in the pack, by its place in it, with `/` between folders.
    pub files: Vec<PackFile>,
    /// The files' bytes together; a zip adds about a hundred bytes a file.
    pub total_bytes: u64,
    /// References the style could not write, so the reading list leaves them
    /// out: their zero-based places in `items`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_in_list: Vec<usize>,
    /// References without a usable citation key, which the `.bib` leaves out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_in_bibtex: Vec<usize>,
    /// With PDFs asked for, references none of whose PDFs could be added: it
    /// has none, or they are outside its library's folder, missing, linked,
    /// not on this computer or over 256 MiB.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub without_pdf: Vec<usize>,
}
impl ShareResult {
    /// Every reference is in every file it was asked for.
    pub fn complete(&self) -> bool {
        self.not_in_list.is_empty() && self.not_in_bibtex.is_empty() && self.without_pdf.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackFile {
    pub path: String,
    pub bytes: u64,
}

enum Content {
    Bytes(Vec<u8>),
    /// A PDF at this location, of this size when the plan opened it.
    Pdf(PathBuf, u64),
}

impl Engine {
    /// Plan the pack for `request`, and write it when it names a destination.
    pub fn share(
        &self,
        request: &ShareRequest,
        cancel: &Cancellation,
    ) -> Result<health::Checked<ShareResult>, EngineError> {
        cancel.check()?;
        if request.items.is_empty() {
            return Err(error(ErrorCode::BadInput));
        }
        ibis_core::citation::check_item_limit(request.items.len()).map_err(|_| error(ErrorCode::ResourceLimit))?;
        if request.destination.as_deref().is_some_and(|path| !path.is_absolute()) {
            return Err(EngineError::because(
                ErrorCode::BadInput,
                ErrorReason::DestinationUnwritable,
            ));
        }
        let state = self.state()?;
        let snapshot = state.index.snapshot();
        // Each reference once, at the first place it was asked for.
        let mut picks: Vec<(usize, SnapshotObject)> = Vec::new();
        for (index, selector) in request.items.iter().enumerate() {
            let selected = select(snapshot, &state.libraries, selector)?;
            if !matches!(selected.object, LibraryObject::Record(_)) {
                return Err(error(ErrorCode::NotFound));
            }
            if !picks.iter().any(|(_, pick)| pick.object.id() == selected.object.id()) {
                picks.push((index, selected.clone()));
            }
        }
        let every_library = request
            .items
            .iter()
            .any(|selector| matches!(selector, ItemSelector::Key(_)));
        let items: Vec<CitationItem> = picks
            .iter()
            .map(|(_, pick)| CitationItem {
                item: pick.object.id().clone(),
                expected_cite: Some(pick.fingerprints.cite.clone()),
                form: None,
                locator: None,
                prefix: vec![],
                suffix: vec![],
                suppress_author: false,
                annotation: None,
                expected_annotation_cite: None,
            })
            .collect();
        let listed = self.prepare_consulting(
            &CitationRequest {
                strict: false,
                items: items.clone(),
                keep_order: false,
                output: OutputSpec::Csl {
                    style: request
                        .style
                        .clone()
                        .unwrap_or_else(|| crate::outputs::DEFAULT_STYLE.into()),
                },
                form: CitationForm::BibliographyOnly,
                input_language: None,
                output_locale: request.output_locale.clone(),
            },
            every_library,
            cancel,
        )?;
        let source_health = listed.source_health().clone();
        let fields: Vec<CitableFields> = picks
            .iter()
            .map(|(_, pick)| match &pick.object {
                LibraryObject::Record(record) => CitableFields::new(record.csl.to_map()),
                _ => CitableFields::new(serde_json::Map::new()),
            })
            .collect();
        let bibtex = match self.prepare_citation(
            &CitationRequest {
                strict: false,
                items,
                keep_order: true,
                output: crate::outputs::resolve(&OutputSpec::Preset { name: "bibtex".into() })?,
                form: CitationForm::Normal,
                input_language: None,
                output_locale: None,
            },
            cancel,
        ) {
            Ok(bibtex) => Some(bibtex.into_result()),
            // No reference has a usable key: the pack goes without a `.bib`.
            Err(failure) if failure.citation_error.is_some() => None,
            Err(failure) => return Err(failure),
        };
        let in_bibtex = bibtex.as_ref().map(rendered).unwrap_or_default();
        let bibtex = bibtex
            .map(|bibtex| bibtex.text)
            .filter(|text| !in_bibtex.is_empty() && !text.trim().is_empty())
            .map(|mut text| {
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                text
            });
        let mut pdf_files = Vec::new();
        let mut without_pdf = Vec::new();
        if request.pdfs {
            let found = self.pdfs(&state, &picks, cancel)?;
            let mut taken = HashSet::new();
            for ((index, _), (pdfs, fields)) in picks.iter().zip(found.into_iter().zip(&fields)) {
                if pdfs.is_empty() {
                    without_pdf.push(*index);
                }
                let stem = pdf_stem(fields);
                for (path, bytes) in pdfs {
                    let name = unique(&stem, &mut taken);
                    pdf_files.push((format!("{PDF_FOLDER}/{name}"), Content::Pdf(path, bytes)));
                }
            }
        }
        let (page, listed_items) = reading_list(listed.result(), &fields, bibtex.is_some(), !pdf_files.is_empty())?;
        let mut files = vec![(READING_LIST.to_owned(), Content::Bytes(page.into_bytes()))];
        if let Some(text) = bibtex {
            files.push((BIBTEX_FILE.to_owned(), Content::Bytes(text.into_bytes())));
        }
        files.extend(pdf_files);
        let place = |rendered: &[usize]| -> Vec<usize> {
            picks
                .iter()
                .enumerate()
                .filter(|(at, _)| !rendered.contains(at))
                .map(|(_, (index, _))| *index)
                .collect()
        };
        let packed: Vec<PackFile> = files
            .iter()
            .map(|(path, content)| PackFile {
                path: path.clone(),
                bytes: match content {
                    Content::Bytes(bytes) => bytes.len() as u64,
                    Content::Pdf(_, bytes) => *bytes,
                },
            })
            .collect();
        let total_bytes = packed.iter().map(|file| file.bytes).sum();
        if total_bytes > MAX_PACK_BYTES || packed.len() > MAX_PACK_FILES {
            return Err(error(ErrorCode::ResourceLimit));
        }
        let mut result = ShareResult {
            written: false,
            files: packed,
            total_bytes,
            not_in_list: place(&listed_items),
            not_in_bibtex: place(&in_bibtex),
            without_pdf,
        };
        ensure_budget(&result)?;
        if let Some(destination) = &request.destination {
            cancel.check()?;
            if request.folder {
                write_folder(destination, &files, cancel)?;
            } else {
                write_zip(destination, &files, cancel)?;
            }
            result.written = true;
        }
        Ok(health::Checked {
            source_health,
            value: result,
        })
    }

    /// For each pick, where its PDFs are and their sizes: only files its
    /// library keeps inside its folder or Zotero's storage, that open as
    /// local PDFs. A library file is read once for all its picks.
    fn pdfs(
        &self,
        state: &State,
        picks: &[(usize, SnapshotObject)],
        cancel: &Cancellation,
    ) -> Result<Vec<Vec<(PathBuf, u64)>>, EngineError> {
        let cancelled: Arc<dyn Fn() -> bool + Send + Sync> = {
            let cancel = cancel.clone();
            Arc::new(move || cancel.is_cancelled())
        };
        let mut found: Vec<Vec<PathBuf>> = vec![Vec::new(); picks.len()];
        let mut by_file: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
        for (at, (_, pick)) in picks.iter().enumerate() {
            cancel.check()?;
            // A library Ibis cannot reach now simply gives no PDFs.
            let Ok(owner) = self.owner(state, pick) else { continue };
            match &owner {
                Owner::File(path) => by_file.entry(path.clone()).or_default().push(at),
                Owner::Zotero(reading, _) => {
                    for attachment in self.listed(state, &owner, pick).unwrap_or_default() {
                        if attachment.is_pdf()
                            && !attachment.link_only
                            && let Ok(Some(path)) = reading.file(&attachment, false)
                        {
                            found[at].push(path);
                        }
                    }
                }
                Owner::Snapshot => {}
            }
        }
        for (library, members) in by_file {
            cancel.check()?;
            let parents: Vec<_> = members
                .iter()
                .map(|&at| (picks[at].1.object.id(), &picks[at].1.fingerprints.full))
                .collect();
            let located = ibis_sources::attachment_files(&library, &parents, false, &|| cancel.is_cancelled())
                .map_err(|failure| match failure {
                    // The library changed since it was read: try again once it settles.
                    ibis_sources::SourceError::Changing => error(ErrorCode::NotFound),
                    failure => sources::source_error(failure),
                })?;
            for (&at, attachments) in members.iter().zip(located) {
                let pdf_keys: Vec<String> = self
                    .listed(state, &Owner::Snapshot, &picks[at].1)?
                    .into_iter()
                    .filter(|attachment| attachment.is_pdf() && !attachment.link_only)
                    .map(|attachment| attachment.key)
                    .collect();
                for (key, file) in attachments {
                    if pdf_keys.contains(&key)
                        && let Ok(Some(path)) = file
                    {
                        found[at].push(path);
                    }
                }
            }
        }
        let mut sized = Vec::with_capacity(found.len());
        for paths in found {
            let mut pdfs = Vec::new();
            for path in paths {
                cancel.check()?;
                if pdfs.iter().any(|(seen, _)| *seen == path) {
                    continue;
                }
                if let Ok(pdf) = LocalPdf::open(&path, cancelled.clone()) {
                    let bytes = pdf.len();
                    pdfs.push((path, bytes));
                }
            }
            sized.push(pdfs);
        }
        cancel.check()?;
        Ok(sized)
    }
}

/// The places in the request of the items a result rendered.
fn rendered(result: &CitationResult) -> Vec<usize> {
    result
        .outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            ItemOutcome::Rendered { item } => Some(*item),
            _ => None,
        })
        .collect()
}

/// The reading list page, and the picks it lists. Entries come from the
/// formatter's display tree, so the page holds only text this writer escapes
/// and links it builds from checked DOIs and public URLs.
fn reading_list(
    list: &CitationResult,
    fields: &[CitableFields],
    bibtex: bool,
    pdfs: bool,
) -> Result<(String, Vec<usize>), EngineError> {
    let mut order: Vec<usize> = Vec::new();
    let mut entries: BTreeMap<usize, String> = BTreeMap::new();
    let listed = rendered(list);
    for node in &list.display.nodes {
        let Some(item) = node.item.filter(|item| listed.contains(item)) else {
            continue;
        };
        if !order.contains(&item) {
            order.push(item);
        }
        let entry = entries.entry(item).or_default();
        for run in &node.content {
            run_html(entry, run);
        }
    }
    if order.is_empty() {
        return Err(EngineError::citation(CitationError::UnsupportedForm));
    }
    let words = |id: &str| crate::catalogue::text(id, None, &[]).unwrap_or_default();
    let title = words("app-share-page-title");
    let mut page = String::from("<!doctype html>\n<html");
    if let Some(language) = list
        .language
        .as_deref()
        .filter(|language| language.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    {
        page.push_str(&format!(" lang=\"{language}\""));
    }
    page.push_str(">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>");
    escape(&mut page, &title);
    page.push_str("</title>\n<style>\n");
    page.push_str(PAGE_STYLE);
    page.push_str("</style>\n</head>\n<body>\n<main>\n<h1>");
    escape(&mut page, &title);
    page.push_str("</h1>\n<ol>\n");
    for item in &order {
        let mut entry = entries.remove(item).unwrap_or_default();
        if let Some(link) = fields.get(*item).and_then(link) {
            let mut shown = String::new();
            escape(&mut shown, link);
            let anchor = format!("<a href=\"{shown}\">{shown}</a>");
            match entry.find(&shown) {
                Some(at) => entry.replace_range(at..at + shown.len(), &anchor),
                None => {
                    entry.push(' ');
                    entry.push_str(&anchor);
                }
            }
        }
        page.push_str("<li>");
        page.push_str(entry.trim());
        page.push_str("</li>\n");
    }
    page.push_str("</ol>\n");
    // Say only what the pack holds.
    let notes: Vec<String> = [(bibtex, "app-share-page-import"), (pdfs, "app-share-page-pdfs")]
        .into_iter()
        .filter(|(held, _)| *held)
        .map(|(_, id)| words(id))
        .collect();
    if !notes.is_empty() {
        page.push_str("<p>");
        escape(&mut page, &notes.join(" "));
        page.push_str("</p>\n");
    }
    page.push_str("</main>\n</body>\n</html>\n");
    Ok((page, order))
}

const PAGE_STYLE: &str = ":root { color-scheme: light dark; }\n\
body { margin: 0; background: Canvas; color: CanvasText; font: 17px/1.55 Georgia, \"Times New Roman\", serif; }\n\
main { max-width: 44rem; margin: 0 auto; padding: 3rem 1rem; }\n\
h1 { font: 600 1.6rem/1.2 system-ui, sans-serif; margin: 0 0 2rem; }\n\
ol { list-style: none; margin: 0; padding: 0; }\n\
li { margin: 0 0 1rem; padding-left: 2rem; text-indent: -2rem; }\n\
a { color: LinkText; overflow-wrap: anywhere; }\n\
p { margin-top: 2.5rem; font: 0.9rem/1.45 system-ui, sans-serif; opacity: 0.75; }\n";

/// The reference's DOI link, or else its public address.
fn link(fields: &CitableFields) -> Option<&str> {
    fields
        .doi()
        .map(ibis_core::citable::DoiLink::as_str)
        .or_else(|| fields.plain().get("URL").and_then(Value::as_str))
}

fn run_html(out: &mut String, run: &DisplayRun) {
    let mut tags: Vec<&str> = Vec::new();
    if run.italic {
        tags.push("i");
    }
    if run.bold {
        tags.push("b");
    }
    if run.underline {
        tags.push("u");
    }
    match run.script {
        Script::Superscript => tags.push("sup"),
        Script::Subscript => tags.push("sub"),
        Script::Normal => {}
    }
    for tag in &tags {
        out.push_str(&format!("<{tag}>"));
    }
    if run.small_caps {
        out.push_str("<span style=\"font-variant: small-caps\">");
    }
    escape(out, &run.text);
    if run.small_caps {
        out.push_str("</span>");
    }
    for tag in tags.iter().rev() {
        out.push_str(&format!("</{tag}>"));
    }
}

fn escape(out: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            character if ibis_core::model::is_hidden(character) => {}
            character => out.push(character),
        }
    }
}

/// `Aster 2026 - Title`: the first author's or editor's family name, the
/// year and the title, as a name every system accepts.
fn pdf_stem(fields: &CitableFields) -> String {
    let person = ["author", "editor"].into_iter().find_map(|role| {
        let name = fields.names(role).first()?;
        name.family.clone().or_else(|| name.literal.clone())
    });
    let issued = fields.plain().get("issued");
    let year = issued
        .and_then(|issued| issued.get("date-parts")?.get(0)?.get(0)?.as_i64())
        .map(|year| year.to_string())
        .or_else(|| {
            ["literal", "raw"].into_iter().find_map(|part| {
                let text = issued?.get(part)?.as_str()?;
                ibis_core::citable::literal_year(text).map(|(year, _)| year.to_string())
            })
        });
    let title = fields.plain().get("title").and_then(Value::as_str);
    let head = [person, year].into_iter().flatten().collect::<Vec<_>>().join(" ");
    let stem = match (head.is_empty(), title) {
        (false, Some(title)) => format!("{head} - {title}"),
        (true, Some(title)) => title.to_owned(),
        (_, None) => head,
    };
    file_name(&stem)
}

/// `stem` cleaned for macOS, Windows and Linux alike: no separators, marks
/// Windows refuses, control or hidden characters, leading or trailing dots
/// and spaces, or reserved device names, and at most [`MAX_NAME_BYTES`].
fn file_name(stem: &str) -> String {
    let cleaned: String = stem
        .nfc()
        .filter(|character| !ibis_core::model::is_hidden(*character))
        .map(|character| {
            if character.is_control() || matches!(character, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                ' '
            } else {
                character
            }
        })
        .collect();
    let mut capped = String::new();
    for word in cleaned.split_whitespace() {
        let space = usize::from(!capped.is_empty());
        if capped.len() + space + word.len() > MAX_NAME_BYTES {
            if capped.is_empty() {
                for character in word.chars() {
                    if capped.len() + character.len_utf8() > MAX_NAME_BYTES {
                        break;
                    }
                    capped.push(character);
                }
            }
            break;
        }
        if space == 1 {
            capped.push(' ');
        }
        capped.push_str(word);
    }
    let trimmed = capped.trim_matches(|character: char| character == '.' || character.is_whitespace());
    let name = if trimmed.is_empty() { "Reference" } else { trimmed };
    let device = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && device.as_bytes()[3].is_ascii_digit());
    if reserved { format!("{name}_") } else { name.to_owned() }
}

/// `stem.pdf`, or `stem (2).pdf` and on when a name already in the pack is
/// the same to a system that ignores case or composition.
fn unique(stem: &str, taken: &mut HashSet<String>) -> String {
    let mut number = 1;
    loop {
        let name = if number == 1 {
            format!("{stem}.pdf")
        } else {
            format!("{stem} ({number}).pdf")
        };
        if taken.insert(name.nfc().flat_map(char::to_lowercase).collect()) {
            return name;
        }
        number += 1;
    }
}

fn written(failure: &std::io::Error) -> EngineError {
    EngineError::because(
        ErrorCode::BadInput,
        if failure.kind() == std::io::ErrorKind::AlreadyExists {
            ErrorReason::DestinationExists
        } else {
            ErrorReason::DestinationUnwritable
        },
    )
}

fn create_new(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path)
}

fn destination_error(destination: &Path, failure: &std::io::Error) -> EngineError {
    // Windows can deny a file open when the existing destination is a directory.
    if failure.kind() == std::io::ErrorKind::PermissionDenied && destination.symlink_metadata().is_ok() {
        return EngineError::because(ErrorCode::BadInput, ErrorReason::DestinationExists);
    }
    written(failure)
}

/// Copy a planned PDF, refused if it is no longer the file the plan opened.
fn copy_pdf(path: &Path, bytes: u64, out: &mut impl Write, cancel: &Cancellation) -> Result<(), EngineError> {
    let changed = || error(ErrorCode::SourceUnavailable);
    let cancelled = cancel.clone();
    let pdf = LocalPdf::open(path, Arc::new(move || cancelled.is_cancelled())).map_err(|_| changed())?;
    if pdf.len() != bytes {
        return Err(changed());
    }
    pdf.copy_to(out).map_err(|_| changed())?;
    Ok(())
}

/// One zip file, created new; stored, not compressed: PDFs already are.
fn write_zip(destination: &Path, files: &[(String, Content)], cancel: &Cancellation) -> Result<(), EngineError> {
    let file = create_new(destination).map_err(|failure| destination_error(destination, &failure))?;
    let outcome = (|| {
        let unwritable = || EngineError::because(ErrorCode::BadInput, ErrorReason::DestinationUnwritable);
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        for (name, content) in files {
            cancel.check()?;
            zip.start_file(name.as_str(), options).map_err(|_| unwritable())?;
            match content {
                Content::Bytes(bytes) => zip.write_all(bytes).map_err(|failure| written(&failure))?,
                Content::Pdf(path, bytes) => copy_pdf(path, *bytes, &mut zip, cancel)?,
            }
        }
        cancel.check()?;
        zip.finish()
            .map_err(|_| unwritable())?
            .sync_all()
            .map_err(|failure| written(&failure))
    })();
    if outcome.is_err() {
        let _ = std::fs::remove_file(destination);
    }
    outcome
}

/// A new folder holding the files; on failure, what was made is removed.
fn write_folder(destination: &Path, files: &[(String, Content)], cancel: &Cancellation) -> Result<(), EngineError> {
    std::fs::create_dir(destination).map_err(|failure| destination_error(destination, &failure))?;
    let mut made: Vec<PathBuf> = Vec::new();
    let mut folders: Vec<PathBuf> = vec![destination.to_path_buf()];
    let outcome = (|| {
        for (name, content) in files {
            cancel.check()?;
            let mut path = destination.to_path_buf();
            let mut parts = name.split('/').peekable();
            while let Some(part) = parts.next() {
                path.push(part);
                if parts.peek().is_some() && !folders.contains(&path) {
                    std::fs::create_dir(&path).map_err(|failure| written(&failure))?;
                    folders.push(path.clone());
                }
            }
            let mut file = create_new(&path).map_err(|failure| written(&failure))?;
            made.push(path);
            match content {
                Content::Bytes(bytes) => file.write_all(bytes).map_err(|failure| written(&failure))?,
                Content::Pdf(path, bytes) => copy_pdf(path, *bytes, &mut file, cancel)?,
            }
            file.sync_all().map_err(|failure| written(&failure))?;
        }
        Ok(())
    })();
    if outcome.is_err() {
        for file in made.iter().rev() {
            let _ = std::fs::remove_file(file);
        }
        for folder in folders.iter().rev() {
            let _ = std::fs::remove_dir(folder);
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_suit_every_system_and_never_collide() {
        assert_eq!(
            file_name("Aster 2026 - Lanterns: a/b\\c?"),
            "Aster 2026 - Lanterns a b c"
        );
        assert_eq!(file_name("  ..CON  "), "CON_");
        assert_eq!(file_name("com1.draft"), "com1.draft_");
        assert_eq!(file_name("Console"), "Console");
        // A direction override could disguise `fdp.exe` as `exe.pdf`.
        assert_eq!(file_name("\u{202e}..."), "Reference");
        assert_eq!(file_name("Aster\u{202e}fdp.exe"), "Asterfdp.exe");
        let long = file_name(&"é".repeat(400));
        assert!(long.len() <= MAX_NAME_BYTES && long.chars().all(|c| c == 'é'));
        let mut taken = HashSet::new();
        // Composed and decomposed, upper and lower case are one name on macOS and Windows.
        assert_eq!(unique("Café", &mut taken), "Café.pdf");
        assert_eq!(unique("cafe\u{301}", &mut taken), "cafe\u{301} (2).pdf");
        assert_eq!(unique("CAFÉ", &mut taken), "CAFÉ (3).pdf");
    }
}
