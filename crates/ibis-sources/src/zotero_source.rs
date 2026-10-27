//! Raw live acquisition and immutable publication through the shared mapper.
use crate::{
    LoadedLibrary, SourceWarning, WarningCode,
    zotero::{self, MappingError, MappingWarning, ZoteroCreator, ZoteroItem},
};
use ibis_core::contract::{LibraryId, LibraryObject, Snapshot};
use ibis_core::zotero::api::{Transport, TransportError};
pub use ibis_zotero_local::DEFAULT_PORT;
use ibis_zotero_local::live::{Library, LiveError, LiveReader};
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ZoteroError {
    #[error(transparent)]
    Live(#[from] LiveError),
    #[error("Zotero returned invalid bibliographic data")]
    InvalidData,
    #[error("reading Zotero exceeded the work limit")]
    Limit,
}
impl ZoteroError {
    pub fn cancelled(self) -> bool {
        matches!(self, Self::Live(LiveError::Cancelled))
    }
    pub fn limited(self) -> bool {
        matches!(
            self,
            Self::Limit | Self::Live(LiveError::Limit | LiveError::Transport(TransportError::TooLarge))
        )
    }
}
#[derive(Clone)]
pub struct ZoteroSource {
    reader: LiveReader,
    current: Option<Arc<LoadedLibrary>>,
}
impl Default for ZoteroSource {
    fn default() -> Self {
        Self {
            reader: LiveReader::new(Library::User),
            current: None,
        }
    }
}
impl ZoteroSource {
    pub fn current(&self) -> Option<&Arc<LoadedLibrary>> {
        self.current.as_ref()
    }
    pub fn refresh_local(
        &mut self,
        port: u16,
        reload: bool,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        self.refresh(&mut ibis_zotero_local::LocalZotero::on_port(port), reload, cancelled)
    }
    pub fn refresh(
        &mut self,
        transport: &mut impl Transport,
        reload: bool,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        let mut reader = self.reader.clone();
        reader.refresh(transport, reload, cancelled)?;
        let library = LibraryId::zotero(b"default:user");
        let mut objects = Vec::new();
        let mut warnings = Vec::new();
        for raw in reader.items() {
            if cancelled() {
                return Err(LiveError::Cancelled.into());
            }
            let item = ZoteroItem {
                key: raw.key.clone(),
                item_type: raw.item_type.clone(),
                fields: raw.fields.clone(),
                creators: raw
                    .creators
                    .iter()
                    .map(|creator| ZoteroCreator {
                        creator_type: creator.creator_type.clone(),
                        first_name: creator.first_name.clone(),
                        last_name: creator.last_name.clone(),
                        name: creator.name.clone(),
                        field_mode: creator.field_mode,
                    })
                    .collect(),
                better_bibtex_keys: vec![],
            };
            match zotero::map_item(library.clone(), &item) {
                Ok(mapped) => {
                    objects.push(LibraryObject::Record(mapped.record));
                    warnings.extend(mapped.warnings.into_iter().map(|warning| SourceWarning {
                        code: if warning == MappingWarning::DateParseLimit {
                            WarningCode::WorkLimit
                        } else {
                            WarningCode::Field
                        },
                        line: None,
                    }));
                }
                Err(MappingError::UnsupportedType) => warnings.push(SourceWarning {
                    code: WarningCode::Field,
                    line: None,
                }),
                Err(MappingError::InputLimit) => return Err(ZoteroError::Limit),
                Err(_) => return Err(ZoteroError::InvalidData),
            }
        }
        let snapshot = Snapshot::new(objects).map_err(|_| ZoteroError::InvalidData)?;
        if cancelled() {
            return Err(LiveError::Cancelled.into());
        }
        let loaded = Arc::new(LoadedLibrary {
            library,
            snapshot,
            warnings,
            attachment_paths: Default::default(),
        });
        self.reader = reader;
        self.current = Some(loaded.clone());
        Ok(loaded)
    }
}
