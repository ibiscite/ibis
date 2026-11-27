//! The picker reads engine projections and commits the preview it showed.
use crate::citations::{SessionError, SessionId};
use crate::{Cancellation, Engine, catalogue, format, health, outputs, recents, settings};
use ibis_core::contract::*;
use ibis_core::destination::{AppId, System};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerOpenRequest {
    pub file: Option<String>,
    pub app: Option<String>,
}

impl PickerOpenRequest {
    pub fn library(&self, path: &Path) -> Result<Option<PathBuf>, EngineError> {
        let file = self.file.as_ref().map(PathBuf::from).or_else(|| {
            match &settings::load(path).cites().ok()?.portable.library {
                Some(settings::LibrarySetting::Bibtex(file)) => Some(file.clone()),
                _ => None,
            }
        });
        if file.as_ref().is_some_and(|file| !file.is_absolute()) {
            return Err(ErrorCode::BadInput.into());
        }
        Ok(file)
    }

    pub fn destination(&self) -> Option<AppId> {
        self.app.as_ref().filter(|id| !id.is_empty()).map(|id| AppId {
            system: System::Macos,
            id: id.clone(),
        })
    }
}

#[derive(Serialize, JsonSchema)]
pub struct PickerOpenResult {
    pub library_open: bool,
    pub format: String,
    pub formats: Vec<PickerFormat>,
    pub forms: Vec<PickerForm>,
    pub locators: Vec<PickerLocator>,
    pub library_extensions: Vec<String>,
    pub paste_enabled: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerFormat {
    pub id: String,
    pub name: String,
    pub forms: Vec<CitationForm>,
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
}

pub fn open_result(path: &Path, app: Option<&AppId>, library_open: bool) -> PickerOpenResult {
    let loaded = settings::load(path);
    let defaults = settings::Settings::default();
    let choice = format::usual(loaded.cites().unwrap_or(&defaults), app);
    PickerOpenResult {
        library_open,
        format: choice.id.unwrap_or_else(|| outputs::DEFAULT.to_owned()),
        formats: outputs::outputs()
            .iter()
            .filter(|entry| !entry.exports_records && !entry.zotero_items)
            .map(|entry| PickerFormat {
                id: entry.id.clone(),
                name: entry.name.clone(),
                forms: entry.forms.clone(),
            })
            .collect(),
        forms: CitationForm::ALL
            .into_iter()
            .map(|form| PickerForm {
                form,
                name: label("form", form),
            })
            .collect(),
        locators: LocatorTerm::ALL
            .into_iter()
            .map(|term| PickerLocator {
                term,
                name: label("locator", term),
            })
            .collect(),
        paste_enabled: loaded.cites().is_ok_and(|saved| saved.portable.paste()),
        library_extensions: crate::library_file_extensions().map(str::to_owned).collect(),
    }
}

fn label(group: &str, value: impl Serialize) -> String {
    let id = catalogue::id(group, value);
    catalogue::text(&id, None, &[]).unwrap_or(id)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerSearchRequest {
    pub query: String,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerSearchResult {
    pub rows: Vec<PickerRow>,
    pub heading: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerRow {
    pub id: ItemRef,
    #[schemars(with = "String")]
    pub cite: Fingerprint,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub year: Option<String>,
    pub journal: Option<String>,
}

fn row(item: &SnapshotObject) -> Option<PickerRow> {
    let LibraryObject::Record(record) = &item.object else {
        return None;
    };
    let summary = record.summary();
    Some(PickerRow {
        id: record.id.clone(),
        cite: item.fingerprints.cite.clone(),
        title: summary.title,
        authors: summary.authors.into_iter().map(|author| author.short).collect(),
        year: summary.year,
        journal: summary.container,
    })
}

pub fn search(
    engine: &Engine,
    request: PickerSearchRequest,
    recent: &[ItemRef],
    cancel: &Cancellation,
) -> Result<PickerSearchResult, EngineError> {
    let (rows, health, heading) = if request.query.trim().is_empty() {
        let (snapshot, libraries) = engine.health()?;
        let mut picks = recents::resolve(&snapshot, recent, false);
        let heading = if picks.is_empty() {
            picks = recents::resolve(&snapshot, &[], true);
            "Recently Added"
        } else {
            "Recent Citations"
        };
        let rows = picks.into_iter().filter_map(|(item, _)| row(item)).collect::<Vec<_>>();
        let heading = (!rows.is_empty()).then_some(heading);
        (rows, health::SourceHealth::of(&libraries), heading)
    } else {
        let result = engine.search(
            &crate::request::SearchRequest {
                query: request.query,
                limit: 50,
                libraries: None,
                interpret_locator: false,
            },
            cancel,
        )?;
        let heading = result.hits.first().filter(|hit| hit.partial).map(|_| "Closest Matches");
        (
            result.hits.iter().filter_map(|hit| row(&hit.item)).collect(),
            result.source_health,
            heading,
        )
    };
    let heading = match health.findings.first() {
        Some(health::SourceFinding::RefreshFailed { .. }) => catalogue::text("app-library-not-refreshed", None, &[]),
        Some(health::SourceFinding::SourceWarnings { count, .. }) => {
            Some(format!("{count} library entries need attention."))
        }
        _ => heading.map(str::to_owned),
    };
    Ok(PickerSearchResult { rows, heading })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerPreviewRequest {
    pub item: ItemRef,
    #[schemars(with = "String")]
    pub cite: Fingerprint,
    pub format: String,
    pub locator: Option<LocatorInput>,
    pub form: Option<CitationForm>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerPreviewResult {
    pub ticket: String,
    pub content_hash: String,
    pub text: String,
    pub problem: Option<String>,
}

pub fn preview(
    engine: &Engine,
    request: PickerPreviewRequest,
    cancel: Cancellation,
) -> Result<PickerPreviewResult, SessionError> {
    let result = engine
        .citations()
        .preview(
            SessionId::PICKER,
            Draft {
                items: vec![DraftItem {
                    id: request.item,
                    cite: request.cite,
                    locator: None,
                    locator_input: request.locator,
                    form: request.form,
                    prefix: request.prefix,
                    suffix: request.suffix,
                }],
                output: OutputSpec::Preset { name: request.format },
                form: CitationForm::Normal,
                keep_order: false,
                input_language: None,
                output_locale: None,
                sample: false,
            },
            cancel,
        )?
        .run()?;
    Ok(PickerPreviewResult {
        ticket: result.ticket,
        content_hash: result.content_hash,
        text: result.text,
        problem: result.problem,
    })
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
/// previews, and the settings lease orders paste revocation.
pub fn deliver<T>(
    engine: &Engine,
    settings_path: &Path,
    request: PickerDeliverRequest,
    paste_at_opening: bool,
    write: impl FnOnce(PickerDeliverResult) -> T,
) -> Result<(T, Vec<ItemRef>), SessionError> {
    let checked = engine
        .citations()
        .commit(SessionId::PICKER, &request.ticket, &request.content_hash)?;
    settings::with_paste_consent(settings_path, |paste| {
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
