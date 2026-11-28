//! Projections for the native picker. The shell measures and presents them.
use crate::catalogue::{self, Audience, Language};
use crate::citations::{SessionError, SessionId};
use crate::{Cancellation, Engine, health, outputs, recents, settings};
use ibis_core::contract::*;
use ibis_core::destination::{AppId, System};
use ibis_core::summary::Author;
use ibis_sources::LibraryKind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod format_scope;
mod session;
mod words;
pub use format_scope::{PickerChangeFormatRequest, PickerFormatScope};
pub use session::{PickerDetails, PickerEntry, PickerIntent, PickerSession, PickerState};
pub use words::shell_words;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerOpenRequest {
    pub file: Option<String>,
    pub app: Option<String>,
    pub site: Option<String>,
    pub language: Language,
    #[serde(default)]
    pub reload: bool,
    /// Keep the last search, as when the picker reopens soon after closing.
    #[serde(default)]
    pub keep_query: bool,
}

impl PickerOpenRequest {
    pub fn library(&self, path: &Path) -> Result<Option<settings::LibrarySetting>, EngineError> {
        let library = self
            .file
            .as_ref()
            .map(|file| settings::LibrarySetting::Bibtex(PathBuf::from(file)))
            .or_else(|| settings::load(path).cites().ok()?.portable.library.clone());
        if matches!(&library, Some(settings::LibrarySetting::Bibtex(file)) if !file.is_absolute()) {
            return Err(ErrorCode::BadInput.into());
        }
        Ok(library)
    }

    pub fn destination(&self) -> Option<AppId> {
        self.app.as_ref().filter(|id| !id.is_empty()).map(|id| AppId {
            system: System::Macos,
            id: id.clone(),
        })
    }

    pub fn site(&self) -> Option<AppId> {
        let shown = self.site.as_deref()?;
        self.destination()
            .as_ref()
            .filter(|app| ibis_core::destination::is_browser(app))?;
        ibis_core::destination::site(shown).filter(|site| site.id == shown)
    }
}

#[derive(Serialize, JsonSchema)]
pub struct PickerOpenResult {
    pub library: Option<PickerLibrary>,
    pub status: Vec<PickerStatus>,
    pub format_scope: PickerFormatScope,
    pub formats: Vec<PickerFormat>,
    pub forms: Vec<PickerForm>,
    pub locators: Vec<PickerLocator>,
    pub library_extensions: Vec<String>,
    pub paste_enabled: bool,
    pub appearance: settings::Appearance,
    pub density: settings::Density,
    pub larger_text: bool,
    /// The session as it opens, drawn before the first key.
    pub state: PickerState,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerLibrary {
    pub name: String,
    pub kind: LibraryKind,
    pub condition: PickerLibraryCondition,
    pub file: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PickerLibraryCondition {
    Ready,
    Missing,
    Unreadable,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerFormat {
    pub id: String,
    pub name: String,
    pub forms: Vec<CitationForm>,
    /// A style can use the writer's order instead of its own.
    pub can_keep_order: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerForm {
    pub form: CitationForm,
    pub name: String,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerLocator {
    pub term: LocatorTerm,
    pub name: String,
    pub short_name: String,
}

pub fn open_result(
    path: &Path,
    app: Option<&AppId>,
    site: Option<&AppId>,
    source: Option<&settings::LibrarySetting>,
    error: Option<EngineError>,
    language: Language,
    session: impl FnOnce(&PickerFormatScope, bool) -> PickerState,
) -> PickerOpenResult {
    use settings::LibrarySetting;
    let loaded = settings::load(path);
    let defaults = settings::Settings::default();
    let settings = loaded.cites().unwrap_or(&defaults);
    let library = source.map(|source| {
        let (name, kind, file) = match source {
            LibrarySetting::Bibtex(file) => (
                file.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                LibraryKind::File,
                Some(file.to_string_lossy().into_owned()),
            ),
            LibrarySetting::Zotero(_) => (word(language, "picker-native-zotero", &[]), LibraryKind::Zotero, None),
        };
        let condition = match (source, error) {
            (_, None) => PickerLibraryCondition::Ready,
            (LibrarySetting::Bibtex(file), Some(_)) if file.try_exists().ok() == Some(false) => {
                PickerLibraryCondition::Missing
            }
            (_, Some(_)) => PickerLibraryCondition::Unreadable,
        };
        PickerLibrary {
            name,
            kind,
            condition,
            file,
        }
    });
    let status = error
        .into_iter()
        .zip(&library)
        .map(|(error, library)| {
            let (text, fix) = if library.condition == PickerLibraryCondition::Missing {
                (
                    word(language, "picker-native-library-missing", &[("name", &library.name)]),
                    PickerFix::LocateLibrary,
                )
            } else {
                (problem_text(language, &error.into()), PickerFix::ChooseLibrary)
            };
            PickerStatus {
                kind: PickerStatusKind::Library,
                text,
                fix: Some(fix),
                item: None,
            }
        })
        .collect();
    let format_scope = PickerFormatScope::for_destination(settings, app, site);
    let ready = library
        .as_ref()
        .is_some_and(|library| library.condition == PickerLibraryCondition::Ready);
    let state = session(&format_scope, ready);
    PickerOpenResult {
        library,
        status,
        format_scope,
        state,
        formats: outputs::outputs()
            .iter()
            .filter(|entry| !entry.exports_records && !entry.zotero_items)
            .map(|entry| PickerFormat {
                id: entry.id.clone(),
                name: output_name(entry, language),
                forms: entry.forms.clone(),
                can_keep_order: matches!(entry.output, OutputSpec::Csl { .. }),
            })
            .collect(),
        forms: CitationForm::ALL
            .into_iter()
            .map(|form| PickerForm {
                form,
                name: match form {
                    CitationForm::Narrative => word(language, "picker-native-form-narrative", &[]),
                    _ => word(language, &catalogue::id("form", form), &[]),
                },
            })
            .collect(),
        locators: LocatorTerm::ALL
            .into_iter()
            .map(|term| PickerLocator {
                term,
                name: word(language, &catalogue::id("locator", term), &[]),
                short_name: word(language, &catalogue::id("locator-short", term), &[]),
            })
            .collect(),
        paste_enabled: loaded.cites().is_ok_and(|saved| saved.portable.paste()),
        library_extensions: crate::library_file_extensions().map(str::to_owned).collect(),
        appearance: settings.portable.appearance(),
        density: settings.portable.density(),
        larger_text: settings.portable.larger_text(),
    }
}

fn word(language: Language, id: &str, args: &[(&str, &str)]) -> String {
    catalogue::text_in(language, id, None, args).unwrap_or_else(|| id.to_owned())
}

fn output_name(output: &outputs::OutputInfo, language: Language) -> String {
    let id = output
        .id
        .strip_prefix("china-national-standard-gb-t-7714-2025-")
        .map_or_else(
            || format!("output-name-{}", output.id),
            |edition| format!("output-name-gb-{edition}"),
        );
    catalogue::text_in(language, &id, None, &[]).unwrap_or_else(|| output.name.clone())
}

fn problem_text(language: Language, problem: &Problem) -> String {
    catalogue::message_in(language, &problem.info.message, Audience::App)
}

#[derive(Clone)]
pub(crate) struct PickerSearchResult {
    pub sections: Vec<PickerSection>,
    /// The count of all matches. Absent for an empty search.
    pub count_label: Option<String>,
    pub status: Vec<PickerStatus>,
    pub segments: Vec<PickerSegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PickerSectionKind {
    Best,
    Closest,
    CitedHere,
    RecentlyAdded,
    Pasted,
}

#[derive(Clone, Serialize, JsonSchema)]
pub struct PickerSection {
    pub kind: PickerSectionKind,
    pub rows: Vec<PickerRow>,
}

#[derive(Clone, Serialize, JsonSchema)]
pub struct PickerStatus {
    pub kind: PickerStatusKind,
    pub text: String,
    pub fix: Option<PickerFix>,
    pub item: Option<ItemRef>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PickerStatusKind {
    Library,
    Citation,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PickerFix {
    ChooseLibrary,
    LocateLibrary,
    ReloadLibrary,
    ClearSearch,
    ChooseFormat,
    EditLocator,
    EditCitation,
}

/// One reference as typed: the search, and the prefix and locator read around it.
#[derive(Clone)]
pub(crate) struct PickerSegment {
    pub complete: bool,
    pub locator: Option<Locator>,
    pub prefix: Option<String>,
    /// A finished segment counts only with a clear first match.
    pub row: Option<PickerRow>,
}

#[derive(Clone, PartialEq, Serialize, JsonSchema)]
pub struct PickerRow {
    pub id: ItemRef,
    #[schemars(with = "String")]
    pub cite: Fingerprint,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub first_author: Option<String>,
    pub author_count: usize,
    pub year: Option<String>,
    pub journal: Option<String>,
    pub item_type: Option<String>,
    pub kind: PickerRowKind,
    pub kind_label: String,
    pub citation_key: Option<String>,
    pub full_byline: String,
    /// Author and venue alternatives, best first. The date has its own column.
    pub bylines: Vec<String>,
    /// Localized explanations of the current query match, most important first.
    pub match_reasons: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PickerRowKind {
    Article,
    Book,
    Chapter,
    Thesis,
    Web,
    Report,
    Other,
}

fn kind(item_type: &str) -> PickerRowKind {
    use PickerRowKind::*;
    match item_type {
        "article" | "article-journal" | "article-magazine" | "article-newspaper" => Article,
        "book" => Book,
        "chapter" | "entry" | "entry-dictionary" | "entry-encyclopedia" => Chapter,
        "thesis" => Thesis,
        "webpage" => Web,
        "report" | "standard" => Report,
        _ => Other,
    }
}

fn bylines(authors: &[Author], year: &str, container: Option<&str>, language: Language) -> (String, Vec<String>) {
    let join = |authors: &str, date, venue| {
        [(!authors.is_empty()).then_some(authors), date, venue]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let names = authors
        .iter()
        .map(|author| author.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let full = join(&names, Some(year), container);
    let first = authors.first();
    let and_others = first.map_or_else(String::new, |author| {
        word(language, "picker-native-author-others", &[("author", &author.short)])
    });
    let main = if authors.len() > 2 { &and_others } else { &names };
    let short = if authors.len() > 2 {
        and_others.clone()
    } else {
        authors
            .iter()
            .map(|author| author.short.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut choices = vec![
        join(main, None, container),
        join(&short, None, container),
        join(main, None, None),
        join(&short, None, None),
    ];
    if authors.len() > 1 {
        choices.push(join(&and_others, None, None));
    }
    if let Some(author) = first {
        choices.push(join(&author.short, None, None));
        for (end, _) in author.short.char_indices().filter(|(_, c)| c.is_whitespace()).rev() {
            choices.push(join(&format!("{}…", author.short[..end].trim_end()), None, None));
        }
    }
    choices.retain(|choice| !choice.is_empty());
    choices.dedup();
    (full, choices)
}

fn row(item: &SnapshotObject, language: Language) -> Option<PickerRow> {
    let LibraryObject::Record(record) = &item.object else {
        return None;
    };
    let mut summary = record.full_summary();
    for author in summary.authors.iter_mut().filter(|author| author.others) {
        let label = word(language, "row-et-al", &[]);
        author.name = label.clone();
        author.short = label;
    }
    let no_date = word(language, "picker-no-date", &[]);
    let (full_byline, bylines) = bylines(
        &summary.authors,
        summary.year.as_deref().unwrap_or(&no_date),
        summary.container.as_deref(),
        language,
    );
    let item_type = summary.item_type.as_deref().unwrap_or_default();
    let kind_label = if item_type == "article-journal" {
        word(language, "picker-native-journal-article", &[])
    } else {
        let short = word(language, "row-item-type", &[("type", item_type)]);
        if short.is_empty() {
            word(language, "picker-native-other-reference", &[])
        } else {
            short
        }
    };
    Some(PickerRow {
        id: record.id.clone(),
        cite: item.fingerprints.cite.clone(),
        title: summary.title,
        first_author: summary.authors.first().map(|author| author.short.clone()),
        author_count: summary.authors.len(),
        authors: summary.authors.into_iter().map(|author| author.name).collect(),
        year: summary.year,
        journal: summary.container,
        kind: kind(item_type),
        kind_label,
        item_type: summary.item_type,
        citation_key: record
            .citation_key()
            .ok()
            .flatten()
            .map(str::to_owned)
            // A CSL-JSON record's own id, without the snapshot's prefix.
            .or_else(|| Some(record.id.key.strip_prefix("record:").unwrap_or(&record.id.key).to_owned())),
        full_byline,
        bylines,
        match_reasons: Vec::new(),
    })
}

fn search_row(hit: &crate::service::SearchHit, language: Language) -> Option<PickerRow> {
    use ibis_core::search::MatchField::*;

    let mut row = row(&hit.item, language)?;
    if hit.partial {
        row.match_reasons.push(word(language, "picker-closest-match", &[]));
    } else if hit.typo {
        row.match_reasons.push(word(language, "picker-spelling-match", &[]));
    }
    let visible = hit
        .matched
        .iter()
        .any(|field| matches!(field, Citekey | Title | Author | Year));
    if !visible {
        let field = match hit.matched.first() {
            Some(Doi) => Some("doi"),
            Some(Keyword) => Some("keyword"),
            Some(Container) => Some("container"),
            Some(Abstract) => Some("abstract"),
            Some(Identifier) => Some("identifier"),
            _ => None,
        };
        if let Some(field) = field {
            row.match_reasons
                .push(word(language, "picker-matched-in", &[("field", field)]));
        }
    }
    Some(row)
}

fn source_status(health: health::SourceHealth, language: Language) -> Vec<PickerStatus> {
    health
        .findings
        .into_iter()
        .filter_map(|finding| {
            let (text, fix) = match finding {
                health::SourceFinding::RefreshFailed { .. } => (
                    word(language, "app-library-not-refreshed", &[]),
                    Some(PickerFix::ReloadLibrary),
                ),
                health::SourceFinding::SourceWarnings { count, .. } => (
                    catalogue::count_in(language, "library-entries-to-check", count).unwrap_or_default(),
                    None,
                ),
                health::SourceFinding::Unknown => return None,
            };
            Some(PickerStatus {
                kind: PickerStatusKind::Library,
                text,
                fix,
                item: None,
            })
        })
        .collect()
}

/// One search as typed: `;` separates references, and each reads its own
/// signal and locator (`see vale 417; okafor`). Only the last is listed.
pub(crate) fn search(
    engine: &Engine,
    typed: &str,
    recent: &[ItemRef],
    language: Language,
    cancel: &Cancellation,
) -> Result<PickerSearchResult, EngineError> {
    if typed.len() > 64 * 1024 {
        return Err(ErrorCode::BadInput.into());
    }
    let mut pieces: Vec<&str> = typed.split(';').collect();
    if pieces.len() > crate::MAX_CITATION_ITEMS + 1 {
        return Err(ErrorCode::ResourceLimit.into());
    }
    let query = pieces.pop().unwrap_or_default().trim();
    let mut segments = Vec::new();
    let mut status = Vec::new();
    let labels: Vec<_> = if language == Language::English {
        Vec::new()
    } else {
        LocatorTerm::ALL
            .into_iter()
            .map(|term| (word(language, &catalogue::id("locator", term), &[]), term))
            .collect()
    };
    let search_one = |query: &str| {
        engine.search_with_total(
            &crate::request::SearchRequest {
                query: query.to_owned(),
                limit: 50,
                libraries: None,
                interpret_locator: true,
            },
            &labels,
            cancel,
        )
    };
    let segment = |complete, result: &crate::service::SearchResult| {
        let read = result.shorthand.as_ref();
        let best = result.hits.first().filter(|hit| {
            complete && !hit.partial && !hit.typo && result.hits.get(1).is_none_or(|next| hit.score > next.score)
        });
        PickerSegment {
            complete,
            prefix: read.and_then(|read| read.prefix.clone()),
            locator: read
                .and_then(|read| read.locator.as_ref())
                .map(|found| found.locator.clone()),
            row: best.and_then(|hit| row(&hit.item, language)),
        }
    };
    for typed in pieces.into_iter().map(str::trim).filter(|part| !part.is_empty()) {
        let (found, _) = search_one(typed)?;
        let finished = segment(true, &found);
        if finished.row.is_none() && status.is_empty() {
            status.push(PickerStatus {
                kind: PickerStatusKind::Citation,
                text: word(language, "picker-native-unmatched-segment", &[("query", typed)]),
                fix: Some(PickerFix::ClearSearch),
                item: None,
            });
        }
        segments.push(finished);
    }
    let (sections, total, health) = if query.is_empty() {
        let (snapshot, libraries) = engine.health()?;
        let (cited, added): (Vec<_>, Vec<_>) = recents::resolve_with_added(&snapshot, recent, 12)
            .into_iter()
            .partition(|(_, cited)| *cited);
        let sections = [
            (PickerSectionKind::CitedHere, cited),
            (PickerSectionKind::RecentlyAdded, added),
        ]
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(kind, items)| PickerSection {
            kind,
            rows: items.into_iter().filter_map(|(item, _)| row(item, language)).collect(),
        })
        .collect();
        (sections, None, health::SourceHealth::of(&libraries))
    } else {
        let (result, total) = search_one(query)?;
        segments.push(segment(false, &result));
        let kind = if result.list.is_some() {
            PickerSectionKind::Pasted
        } else if result.hits.first().is_some_and(|hit| hit.partial) {
            PickerSectionKind::Closest
        } else {
            PickerSectionKind::Best
        };
        let rows = result.hits.iter().filter_map(|hit| search_row(hit, language)).collect();
        (vec![PickerSection { kind, rows }], Some(total), result.source_health)
    };
    status.extend(source_status(health, language));
    Ok(PickerSearchResult {
        sections,
        count_label: total.and_then(|count| catalogue::count_in(language, "picker-result-count", count)),
        status,
        segments,
    })
}

#[derive(Clone, Serialize, JsonSchema)]
pub struct PickerPreviewResult {
    pub ticket: String,
    pub content_hash: String,
    pub text: String,
    /// Ordered spans owned by request entries, including the rich text runs.
    pub display: DisplayTree,
    pub warnings: Vec<PickerWarning>,
    pub problem: Option<PickerStatus>,
}

#[derive(Clone, Serialize, JsonSchema)]
pub struct PickerWarning {
    pub item: Option<usize>,
    pub text: String,
}

/// Render the citation the person would deliver, as one ticket.
pub(crate) fn preview(
    engine: &Engine,
    entries: &[PickerEntry],
    format: &str,
    keep_order: bool,
    language: Language,
) -> PickerPreviewResult {
    let result = engine
        .citations()
        .preview(
            SessionId::PICKER,
            Draft {
                items: entries
                    .iter()
                    .map(|entry| DraftItem {
                        id: entry.row.id.clone(),
                        cite: entry.row.cite.clone(),
                        locator: entry.details.locator.clone(),
                        locator_input: None,
                        form: entry.details.form,
                        prefix: entry.details.prefix.clone(),
                        suffix: entry.details.suffix.clone(),
                    })
                    .collect(),
                output: OutputSpec::Preset {
                    name: format.to_owned(),
                },
                form: CitationForm::Normal,
                keep_order,
                input_language: None,
                output_locale: None,
                sample: false,
            },
            Cancellation::default(),
        )
        .and_then(|work| work.run());
    let status = |problem: Problem| {
        let fix = match problem.info.recovery() {
            Recovery::ChooseOutput | Recovery::ChooseStyle => Some(PickerFix::ChooseFormat),
            Recovery::EditLocator => Some(PickerFix::EditLocator),
            Recovery::ChooseForm | Recovery::EditCitation => Some(PickerFix::EditCitation),
            _ => None,
        };
        PickerStatus {
            kind: PickerStatusKind::Citation,
            text: problem_text(language, &problem),
            fix,
            item: problem.info.item.map(|item| *item),
        }
    };
    match result {
        Ok(result) => PickerPreviewResult {
            ticket: result.ticket,
            content_hash: result.content_hash,
            text: result.text,
            display: result.display,
            warnings: result
                .warnings
                .into_iter()
                .map(|warning| PickerWarning {
                    item: warning.item,
                    text: catalogue::message_in(language, &warning.message, Audience::App),
                })
                .collect(),
            problem: result.failure.map(status),
        },
        Err(error) => PickerPreviewResult {
            ticket: String::new(),
            content_hash: String::new(),
            text: String::new(),
            display: DisplayTree::default(),
            warnings: Vec::new(),
            problem: Some(status(error.problem())),
        },
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerFormsRequest {
    pub ticket: String,
    pub content_hash: String,
    pub item: usize,
}

pub fn forms(engine: &Engine, request: PickerFormsRequest, cancel: Cancellation) -> Result<FormDetails, SessionError> {
    engine
        .citations()
        .forms(
            SessionId::PICKER,
            &request.ticket,
            &request.content_hash,
            request.item,
            cancel,
        )?
        .run()
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerDeliverRequest {
    pub ticket: String,
    pub content_hash: String,
    pub paste: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerDeliverResult {
    pub text: String,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub paste: bool,
}

/// The transport keeps this turn until the native writer finishes. The
/// publication lock orders library reloads, the checked content orders
/// previews, and the settings lease keeps the format and paste consent current.
pub fn deliver<T>(
    engine: &Engine,
    settings_path: &Path,
    request: PickerDeliverRequest,
    paste_at_opening: bool,
    format_is_current: impl FnOnce(Option<&settings::Settings>) -> bool,
    write: impl FnOnce(PickerDeliverResult) -> T,
) -> Result<(T, Vec<ItemRef>), SessionError> {
    let checked = engine
        .citations()
        .commit(SessionId::PICKER, &request.ticket, &request.content_hash)?;
    settings::with_delivery_settings(settings_path, |saved| {
        if !format_is_current(saved) {
            return Err(SessionError::SnapshotChanged);
        }
        let paste = saved.is_some_and(|settings| settings.portable.paste());
        let _publication = engine.shared.publication.lock().map_err(|_| ErrorCode::Internal)?;
        let guard = checked.check(engine)?;
        let content = guard.content();
        let result = write(PickerDeliverResult {
            text: content.text.to_owned(),
            html: content.html.map(str::to_owned),
            rtf: content.rtf.map(str::to_owned),
            paste: request.paste && paste_at_opening && paste,
        });
        drop(guard);
        engine.citations().invalidate(SessionId::PICKER);
        Ok((result, checked.citation().items()))
    })
    .map_err(|_| SessionError::from(ErrorCode::SourceUnavailable))?
}

#[cfg(test)]
mod tests;
