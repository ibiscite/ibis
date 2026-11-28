//! The picker session: one per picker connection, owning everything a shell
//! would otherwise keep a copy of (decision 0066). A shell sends intents and
//! draws the state each reply carries; it keeps only windows, focus and keys.
use super::{
    PickerFix, PickerFormatScope, PickerPreviewResult, PickerRow, PickerSearchResult, PickerSection, PickerStatus,
    PickerStatusKind, preview, search, word,
};
use crate::catalogue::{self, Language};
use crate::{Cancellation, Engine, outputs};
use ibis_core::contract::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What the person may change about one reference in the citation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerDetails {
    pub locator: Option<Locator>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub form: Option<CitationForm>,
}

impl PickerDetails {
    fn tidy(mut self) -> Self {
        let keep = |text: Option<String>| text.filter(|text| !text.trim().is_empty());
        self.locator = self.locator.filter(|locator| !locator.value.trim().is_empty());
        self.prefix = keep(self.prefix);
        self.suffix = keep(self.suffix);
        self
    }
}

/// One reference of the citation Return delivers.
#[derive(Clone, PartialEq, Serialize, JsonSchema)]
pub struct PickerEntry {
    pub row: PickerRow,
    pub details: PickerDetails,
    /// Added with Tab or by hand, rather than only typed or highlighted.
    pub added: bool,
    /// The locator as the row shows it, such as "p. 417".
    pub note: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "intent", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PickerIntent {
    /// The search field changed.
    Type {
        query: String,
    },
    Highlight {
        item: ItemRef,
    },
    Move {
        step: i32,
    },
    /// Tab: what is typed and highlighted joins the citation, and the field clears.
    Add,
    /// Add a listed reference, or remove it if it is already in the citation.
    Toggle {
        item: ItemRef,
    },
    Remove {
        item: ItemRef,
    },
    /// Move a reference within the citation; the style then keeps this order.
    Reorder {
        item: ItemRef,
        step: i32,
    },
    Details {
        item: ItemRef,
        details: PickerDetails,
    },
    KeepOrder {
        on: bool,
    },
    /// Use a format for this citation only; `None` returns to the remembered one.
    UseFormat {
        id: Option<String>,
    },
    Undo,
    /// Escape with text: the field clears, the citation stays.
    ClearSearch,
}

#[derive(Serialize, JsonSchema)]
pub struct PickerState {
    /// Rises with every reply, so a shell can drop one that arrives late.
    pub revision: u64,
    pub query: String,
    pub sections: Vec<PickerSection>,
    pub count_label: Option<String>,
    pub highlight: Option<ItemRef>,
    /// How the typed text was read, such as "Page 417 · “see” before".
    pub reads_as: Option<String>,
    /// The citation Return delivers, in order.
    pub citation: Vec<PickerEntry>,
    pub keep_order: bool,
    pub can_keep_order: bool,
    pub can_undo: bool,
    pub format_scope: Option<PickerFormatScope>,
    /// The format the citation is rendered in, and whether it is used once.
    pub format: Option<String>,
    pub format_held: bool,
    /// The forms the chosen format offers.
    pub forms: Vec<CitationForm>,
    pub preview: Option<PickerPreviewResult>,
    pub status: Vec<PickerStatus>,
}

#[derive(Default)]
pub struct PickerSession {
    language: Language,
    ready: bool,
    recent: Vec<ItemRef>,
    scope: Option<PickerFormatScope>,
    held: Option<String>,
    query: String,
    found: Option<PickerSearchResult>,
    highlight: Option<ItemRef>,
    added: Vec<(PickerRow, PickerDetails)>,
    keep_order: bool,
    undo: Option<(Vec<(PickerRow, PickerDetails)>, bool)>,
    rendered: Option<(Vec<PickerEntry>, String, bool, PickerPreviewResult)>,
    problem: Option<PickerStatus>,
    revision: u64,
}

impl PickerSession {
    /// A new opening over a library: the citation starts empty, and the typed
    /// text stays only when the shell asks, as when it reopens soon after.
    pub fn open(
        &mut self,
        engine: &Engine,
        ready: bool,
        scope: PickerFormatScope,
        recent: Vec<ItemRef>,
        language: Language,
        keep_query: bool,
    ) -> PickerState {
        let query = if keep_query {
            std::mem::take(&mut self.query)
        } else {
            String::new()
        };
        let revision = self.revision;
        *self = Self {
            language,
            ready,
            recent,
            scope: Some(scope),
            query,
            revision,
            ..Self::default()
        };
        self.refresh(engine)
    }

    /// After a delivery: nothing is kept for the next opening but the query.
    pub fn delivered(&mut self) {
        self.added.clear();
        self.undo = None;
        self.rendered = None;
        self.query.clear();
    }

    pub fn scope(&self) -> Option<&PickerFormatScope> {
        self.scope.as_ref()
    }

    /// A remembered format changed: the one used once gives way to it.
    pub fn set_scope(&mut self, engine: &Engine, scope: PickerFormatScope) -> PickerState {
        self.scope = Some(scope);
        self.held = None;
        self.refresh(engine)
    }

    pub fn act(&mut self, engine: &Engine, intent: PickerIntent) -> Result<PickerState, EngineError> {
        use PickerIntent::*;
        match intent {
            Type { query } => {
                if query.len() > 64 * 1024 {
                    return Err(ErrorCode::BadInput.into());
                }
                self.query = query;
                self.search(engine)?;
            }
            Highlight { item } => {
                if self.rows().any(|row| row.id == item) {
                    self.highlight = Some(item);
                }
            }
            Move { step } => {
                let rows: Vec<_> = self.rows().map(|row| row.id.clone()).collect();
                if !rows.is_empty() {
                    let current = self
                        .highlight
                        .as_ref()
                        .and_then(|id| rows.iter().position(|row| row == id))
                        .unwrap_or(0) as i64;
                    let next = (current + i64::from(step)).clamp(0, rows.len() as i64 - 1) as usize;
                    self.highlight = Some(rows[next].clone());
                }
            }
            Add => self.add_pending(engine, true)?,
            Toggle { item } => {
                if self.added.iter().any(|(row, _)| row.id == item) {
                    self.save_undo();
                    self.added.retain(|(row, _)| row.id != item);
                } else {
                    let Some(row) = self.rows().find(|row| row.id == item).cloned() else {
                        return Ok(self.state(engine));
                    };
                    self.save_undo();
                    let details = self.typed_details(&row.id);
                    self.added.push((row, details));
                }
            }
            Remove { item } => {
                if self.added.iter().any(|(row, _)| row.id == item) {
                    self.save_undo();
                    self.added.retain(|(row, _)| row.id != item);
                } else if self.highlight.as_ref() == Some(&item) {
                    self.highlight = None;
                }
            }
            Reorder { item, step } => {
                if let Some(index) = self.added.iter().position(|(row, _)| row.id == item) {
                    let next = index as i64 + i64::from(step);
                    if (0..self.added.len() as i64).contains(&next) {
                        self.save_undo();
                        self.added.swap(index, next as usize);
                        self.keep_order = true;
                    }
                }
            }
            Details { item, details } => {
                let details = details.tidy();
                if let Some((_, current)) = self.added.iter_mut().find(|(row, _)| row.id == item) {
                    *current = details;
                } else if self.pending().iter().any(|(row, _, _)| row.id == item) {
                    // Changing a typed or highlighted reference keeps it and
                    // every other reference typed beside it.
                    self.add_pending(engine, false)?;
                    if let Some((_, current)) = self.added.iter_mut().find(|(row, _)| row.id == item) {
                        *current = details;
                    }
                }
            }
            KeepOrder { on } => {
                self.save_undo();
                self.keep_order = on;
            }
            UseFormat { id } => {
                let known = id
                    .as_ref()
                    .is_none_or(|id| outputs::outputs().iter().any(|output| &output.id == id));
                if !known {
                    return Err(ErrorCode::BadInput.into());
                }
                self.held = id;
                let forms = self.forms();
                for (_, details) in &mut self.added {
                    details.form = details.form.filter(|form| forms.contains(form));
                }
            }
            Undo => {
                if let Some((added, keep_order)) = self.undo.take() {
                    self.added = added;
                    self.keep_order = keep_order;
                }
            }
            ClearSearch => {
                self.query.clear();
                self.search(engine)?;
            }
        }
        Ok(self.state(engine))
    }

    /// Search again and redraw, as after opening or a changed scope.
    pub fn refresh(&mut self, engine: &Engine) -> PickerState {
        if let Err(error) = self.search(engine) {
            self.found = None;
            self.problem = Some(PickerStatus {
                kind: PickerStatusKind::Library,
                text: super::problem_text(self.language, &error.into()),
                fix: Some(PickerFix::ReloadLibrary),
                item: None,
            });
        }
        self.state(engine)
    }

    fn search(&mut self, engine: &Engine) -> Result<(), EngineError> {
        self.problem = None;
        if !self.ready {
            self.found = None;
            self.highlight = None;
            return Ok(());
        }
        let found = search(
            engine,
            &self.query,
            &self.recent,
            self.language,
            &Cancellation::default(),
        )?;
        let rows: Vec<_> = found.sections.iter().flat_map(|section| &section.rows).collect();
        if !self
            .highlight
            .as_ref()
            .is_some_and(|id| rows.iter().any(|row| &row.id == id))
        {
            self.highlight = rows.first().map(|row| row.id.clone());
        }
        self.found = Some(found);
        Ok(())
    }

    fn rows(&self) -> impl Iterator<Item = &PickerRow> {
        self.found
            .iter()
            .flat_map(|found| &found.sections)
            .flat_map(|section| &section.rows)
    }

    /// Moves the citation as it stands into what was added and clears the
    /// field. Tab also takes the highlighted result, whatever Return would cite.
    fn add_pending(&mut self, engine: &Engine, highlighted: bool) -> Result<(), EngineError> {
        let mut pending: Vec<_> = self
            .pending()
            .into_iter()
            .filter(|(_, _, added)| !added)
            .map(|(row, details, _)| (row, details))
            .collect();
        if highlighted
            && let Some(row) = self
                .highlight
                .as_ref()
                .and_then(|id| self.rows().find(|row| &row.id == id))
            && !self.added.iter().chain(&pending).any(|(added, _)| added.id == row.id)
        {
            pending.push((row.clone(), self.typed_details(&row.id)));
        }
        if pending.is_empty() {
            return Ok(());
        }
        self.save_undo();
        self.added.extend(pending);
        self.query.clear();
        self.search(engine)
    }

    fn save_undo(&mut self) {
        self.undo = Some((self.added.clone(), self.keep_order));
    }

    /// The typed signal and locator belong to the highlighted reference.
    fn typed_details(&self, item: &ItemRef) -> PickerDetails {
        if self.highlight.as_ref() != Some(item) {
            return PickerDetails::default();
        }
        let current = self
            .found
            .as_ref()
            .and_then(|found| found.segments.last())
            .filter(|segment| !segment.complete);
        PickerDetails {
            locator: current.and_then(|segment| segment.locator.clone()),
            prefix: current.and_then(|segment| segment.prefix.clone()),
            ..PickerDetails::default()
        }
    }

    /// The citation as it stands: what was added, then each finished typed
    /// reference, then the highlighted one while a reference is being typed or
    /// the citation would otherwise be empty.
    fn pending(&self) -> Vec<(PickerRow, PickerDetails, bool)> {
        let mut entries: Vec<_> = self
            .added
            .iter()
            .map(|(row, details)| (row.clone(), details.clone(), true))
            .collect();
        let has = |entries: &[(PickerRow, PickerDetails, bool)], id: &ItemRef| {
            entries.iter().any(|(row, _, _)| &row.id == id)
        };
        let segments = self.found.iter().flat_map(|found| &found.segments);
        for segment in segments.filter(|segment| segment.complete) {
            if let Some(row) = &segment.row
                && !has(&entries, &row.id)
            {
                let details = PickerDetails {
                    locator: segment.locator.clone(),
                    prefix: segment.prefix.clone(),
                    ..PickerDetails::default()
                };
                entries.push((row.clone(), details, false));
            }
        }
        let typing = self
            .found
            .as_ref()
            .and_then(|found| found.segments.last())
            .is_some_and(|segment| !segment.complete);
        if (typing || entries.is_empty())
            && let Some(row) = self
                .highlight
                .as_ref()
                .and_then(|id| self.rows().find(|row| &row.id == id))
            && !has(&entries, &row.id)
        {
            entries.push((row.clone(), self.typed_details(&row.id), false));
        }
        entries
    }

    fn format(&self) -> Option<String> {
        self.held
            .clone()
            .or_else(|| self.scope.as_ref().and_then(|scope| scope.format.clone()))
            .filter(|format| !format.is_empty())
    }

    fn forms(&self) -> Vec<CitationForm> {
        let format = self.format();
        outputs::outputs()
            .iter()
            .find(|output| Some(&output.id) == format.as_ref())
            .map(|output| output.forms.clone())
            .unwrap_or_default()
    }

    fn can_keep_order(&self) -> bool {
        let format = self.format();
        outputs::outputs()
            .iter()
            .find(|output| Some(&output.id) == format.as_ref())
            .is_some_and(|output| matches!(output.output, OutputSpec::Csl { .. }))
    }

    fn reads_as(&self) -> Option<String> {
        let found = self.found.as_ref()?;
        let language = self.language;
        let current = found.segments.last().filter(|segment| !segment.complete);
        let mut parts = Vec::new();
        if let Some(locator) = current.and_then(|segment| segment.locator.as_ref()) {
            let term = word(language, &catalogue::id("locator", locator.term), &[]);
            parts.push(word(
                language,
                "picker-native-read-locator",
                &[("term", &term), ("value", &locator.value)],
            ));
        }
        if let Some(prefix) = current.and_then(|segment| segment.prefix.as_ref()) {
            parts.push(word(language, "picker-native-read-prefix", &[("text", prefix)]));
        }
        let finished = found.segments.iter().filter(|segment| segment.complete).count();
        if finished > 0 {
            parts.extend(catalogue::count_in(language, "picker-native-read-more", finished));
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    fn state(&mut self, engine: &Engine) -> PickerState {
        self.revision += 1;
        let language = self.language;
        let entries: Vec<PickerEntry> = self
            .pending()
            .into_iter()
            .map(|(row, details, added)| PickerEntry {
                note: details.locator.as_ref().map(|locator| {
                    let short = word(language, &catalogue::id("locator-short", locator.term), &[]);
                    format!("{short} {}", locator.value)
                }),
                row,
                details,
                added,
            })
            .collect();
        let format = self.format();
        let bibliography = self.scope.as_ref().is_some_and(|scope| scope.requires_bibliography);
        let keep_order = self.keep_order && entries.len() > 1;
        let preview = match &format {
            Some(format) if !entries.is_empty() && !bibliography => {
                let unchanged = self
                    .rendered
                    .as_ref()
                    .is_some_and(|(drawn, id, order, _)| drawn == &entries && id == format && *order == keep_order);
                if !unchanged {
                    let result = preview(engine, &entries, format, keep_order, language);
                    self.rendered = Some((entries.clone(), format.clone(), keep_order, result));
                }
                self.rendered.as_ref().map(|(_, _, _, result)| result.clone())
            }
            _ => {
                self.rendered = None;
                None
            }
        };
        let mut status: Vec<PickerStatus> = self.problem.iter().cloned().collect();
        if let Some(found) = &self.found {
            status.extend(found.status.iter().cloned());
        }
        PickerState {
            revision: self.revision,
            query: self.query.clone(),
            sections: self
                .found
                .as_ref()
                .map(|found| found.sections.clone())
                .unwrap_or_default(),
            count_label: self.found.as_ref().and_then(|found| found.count_label.clone()),
            highlight: self.highlight.clone(),
            reads_as: self.reads_as(),
            citation: entries,
            keep_order,
            can_keep_order: self.can_keep_order(),
            can_undo: self.undo.is_some(),
            format_scope: self.scope.clone(),
            format,
            format_held: self.held.is_some(),
            forms: self.forms(),
            preview,
            status,
        }
    }
}
