//! Saved record-only .bib objects serve search; only a fresh parse grants readiness.
use super::*;
use crate::generation::{GenerationStore, GenerationWriter, PreparedLibrary, StoreError};
use ibis_core::contract::Fingerprint;

impl SourceSnapshot {
    pub fn source_path(&self) -> &Path {
        &self.source
    }
    pub fn read(path: &Path) -> Result<Self, SourceError> {
        let source = canonical_path(path).map_err(|_| SourceError::Unavailable)?;
        let text = read_library_file(&source)?;
        if canonical_path(path).ok().as_ref() != Some(&source) {
            return Err(SourceError::Changing);
        }
        Ok(Self {
            source,
            text: text.into(),
        })
    }
    /// Explicit Reload keeps two identical canonical observations and the second's bytes.
    pub fn reload(path: &Path, previous: &Snapshot) -> Result<(LoadedLibrary, Self), SourceError> {
        let before = Self::read(path)?;
        std::thread::sleep(SETTLE);
        let observation = Self::read(path)?;
        if before != observation {
            return Err(SourceError::Changing);
        }
        let loaded = parse(&observation.source, &observation.text, true, Some(previous))?;
        Ok((loaded, observation))
    }
    pub fn load(&self) -> Result<LoadedLibrary, SourceError> {
        parse(&self.source, &self.text, false, None)
    }
    fn library(&self) -> LibraryId {
        LibraryId::file(self.source.as_os_str().as_encoded_bytes())
    }
    fn eligible(&self) -> bool {
        self.source
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bib"))
    }
    /// Stored observations and fingerprints never prove what the source contains.
    pub fn hydrate(generation: &PreparedLibrary) -> Result<LoadedLibrary, StoreError> {
        let objects = generation
            .object_fingerprints()
            .map(|(id, _)| {
                let object = generation.get(id)?.ok_or(StoreError::Invalid)?.object;
                if !matches!(object, LibraryObject::Record(_)) {
                    return Err(StoreError::Invalid);
                }
                Ok(object)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(LoadedLibrary {
            library: generation.library().clone(),
            snapshot: Snapshot::new(objects).map_err(|_| StoreError::Invalid)?,
            warnings: vec![],
            attachment_paths: Default::default(),
            best_attachments: Default::default(),
        })
    }
    /// Allocate under the caller's operation lock; subsequent writes use the
    /// store's immutable identity and are invalidated before a newer load starts.
    pub fn writer(
        &self,
        store: &GenerationStore,
        loaded: &LoadedLibrary,
    ) -> Result<Option<GenerationWriter>, StoreError> {
        if !self.eligible()
            || !loaded.warnings.is_empty()
            || loaded.library != self.library()
            || loaded.local_attachment_count() != 0
            || loaded
                .snapshot
                .objects()
                .iter()
                .any(|object| !matches!(object.object, LibraryObject::Record(_)))
        {
            return Ok(None);
        }
        store
            .begin(loaded.library.clone(), Fingerprint::of_bytes(self.text.as_bytes()))
            .map(Some)
    }
}
