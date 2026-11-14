//! External reads fail closed; only desktop code can ask for the source view.
use super::*;
use crate::settings;

/// One engine gateway, shared by typed reads, dispatch, RPC and MCP.
pub struct ExternalReads<'a> {
    engine: &'a Engine,
    owner: Option<&'a settings::Owner>,
}

impl Engine {
    pub fn external_reads<'a>(&'a self, owner: Option<&'a settings::Owner>) -> ExternalReads<'a> {
        ExternalReads { engine: self, owner }
    }
}

impl ExternalReads<'_> {
    fn records_only(&self) {
        if let Some(owner) = self.owner {
            // Consume authority only through its owner. No declaration or safe
            // passage-text seam exists yet: even a valid primary grants nothing.
            // Missing files, backup recovery and a closed queue cannot grant it.
            let _ = owner.read_with(|loaded| {
                let _ = (loaded.primary_read, loaded.cites());
            });
        }
    }

    pub fn snapshot(&self) -> Result<Arc<Snapshot>, EngineError> {
        self.records_only();
        let source = self.engine.desktop_snapshot()?;
        let kept: Vec<_> = source
            .objects()
            .iter()
            .enumerate()
            .filter_map(|(position, item)| permitted(item).then_some(position))
            .collect();
        if kept.len() == source.objects().len() {
            return Ok(source);
        }
        // Keep source objects and their citation validators intact. Do not
        // rewrite a raw record and pretend its new fields have the old hash.
        Snapshot::new_reusing(vec![], &source, &kept)
            .map(Arc::new)
            .map_err(|_| error(ErrorCode::Internal))
    }

    pub fn object(&self, item: &ItemRef) -> Result<SnapshotObject, EngineError> {
        Ok(self
            .show(&ShowRequest {
                item: ItemSelector::Id(item.clone()),
            })?
            .value)
    }

    pub fn search(&self, request: &SearchRequest, cancel: &Cancellation) -> Result<SearchResult, EngineError> {
        self.records_only();
        let mut result = self.engine.desktop_search(request, cancel)?;
        result.hits.retain(|hit| permitted(&hit.item));
        Ok(result)
    }

    pub fn show(&self, request: &ShowRequest) -> Result<health::Checked<SnapshotObject>, EngineError> {
        self.records_only();
        let result = self.engine.desktop_show_checked(request)?;
        if !permitted(&result.value) {
            return Err(error(ErrorCode::NotFound));
        }
        Ok(result)
    }
}

/// Until the owners supply opt-in publication, admit only records already in
/// the existing bibliographic sharing projection. Everything else is withheld.
fn permitted(item: &SnapshotObject) -> bool {
    let LibraryObject::Record(record) = &item.object else {
        return false;
    };
    let fields = record.csl.to_map();
    ibis_core::citable::share_fields(fields.clone()) == fields
}
