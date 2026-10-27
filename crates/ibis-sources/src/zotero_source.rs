//! Raw live acquisition and immutable publication through the shared mapper.
use crate::{
    LoadedLibrary, SourceWarning, WarningCode,
    zotero::{self, MappingError, MappingWarning, ZoteroCreator, ZoteroItem},
};
use ibis_core::contract::{LibraryId, LibraryObject, Snapshot};
use ibis_core::zotero::api::{Transport, TransportError};
pub use ibis_zotero_local::DEFAULT_PORT;
pub use ibis_zotero_local::live::LiveError;
use ibis_zotero_local::live::{Library, LiveReader};
use ibis_zotero_local::{closed, discovery};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ZoteroError {
    #[error(transparent)]
    Live(#[from] LiveError),
    #[error("Zotero returned invalid bibliographic data")]
    InvalidData,
    #[error(transparent)]
    Closed(#[from] closed::ClosedError),
    #[error("the default Zotero directory is unavailable")]
    Location,
}
impl ZoteroError {
    pub fn cancelled(self) -> bool {
        matches!(
            self,
            Self::Live(LiveError::Cancelled) | Self::Closed(closed::ClosedError::Cancelled)
        )
    }
    pub fn newer(self) -> bool {
        matches!(self, Self::Closed(closed::ClosedError::TooNew))
    }
    pub fn limited(self) -> bool {
        matches!(
            self,
            Self::Closed(closed::ClosedError::TooLarge)
                | Self::Live(LiveError::Limit | LiveError::Transport(TransportError::TooLarge))
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoteroMode {
    Live,
    Closed,
}
impl ZoteroMode {
    pub fn capabilities(self) -> &'static [&'static str] {
        &[
            "bibliographic-records",
            "native-citation-keys",
            "extra-citation-keys",
            "key-aliases",
            "key-conflicts",
        ]
    }
}
pub fn library_id() -> LibraryId {
    LibraryId::zotero(b"default:user")
}
#[derive(Clone)]
pub struct ZoteroSource {
    reader: LiveReader,
    current: Option<Arc<LoadedLibrary>>,
    mode: Option<ZoteroMode>,
}
impl Default for ZoteroSource {
    fn default() -> Self {
        Self {
            reader: LiveReader::new(Library::User),
            current: None,
            mode: None,
        }
    }
}
impl ZoteroSource {
    pub fn current(&self) -> Option<&Arc<LoadedLibrary>> {
        self.current.as_ref()
    }
    pub fn mode(&self) -> Option<ZoteroMode> {
        self.mode
    }
    pub fn refresh_local(
        &mut self,
        port: u16,
        reload: bool,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        self.refresh_observed(port, reload, cancelled, &mut || Ok(()))
    }
    pub fn refresh_observed(
        &mut self,
        port: u16,
        reload: bool,
        cancelled: &impl Fn() -> bool,
        on_full: &mut impl FnMut() -> Result<(), LiveError>,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        self.refresh_live(
            &mut ibis_zotero_local::LocalZotero::on_port(port),
            reload,
            cancelled,
            on_full,
        )
    }
    /// Only the closed reader's own process/locality/copy checks establish that
    /// the discovered directory can be read; API failure establishes none of it.
    pub fn refresh_native(
        &mut self,
        reload: bool,
        cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
        on_full: &mut impl FnMut() -> Result<(), LiveError>,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        match self.refresh_observed(DEFAULT_PORT, reload, &|| cancelled(), on_full) {
            Ok(loaded) => return Ok(loaded),
            Err(ZoteroError::Live(LiveError::Transport(TransportError::Unreachable))) => (),
            Err(error) => return Err(error),
        }
        on_full()?;
        if cancelled() {
            return Err(LiveError::Cancelled.into());
        }
        let data = default_directory()?;
        let acquired = read_closed_at(&data, &std::env::temp_dir(), cancelled.clone())?;
        self.publish_closed(&acquired, &|| cancelled())
    }
    pub fn refresh(
        &mut self,
        transport: &mut impl Transport,
        reload: bool,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        self.refresh_live(transport, reload, cancelled, &mut || Ok(()))
    }
    fn refresh_live(
        &mut self,
        transport: &mut impl Transport,
        reload: bool,
        cancelled: &impl Fn() -> bool,
        on_full: &mut impl FnMut() -> Result<(), LiveError>,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        let mut reader = self.reader.clone();
        reader.refresh_observed(transport, reload, cancelled, on_full)?;
        let loaded = self.map_items(
            reader.items().into_iter().map(|raw| ZoteroItem {
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
            }),
            vec![
                SourceWarning {
                    code: WarningCode::WorkLimit,
                    line: None
                };
                reader.omitted_items()
            ],
            cancelled,
        )?;
        self.reader = reader;
        self.current = Some(loaded.clone());
        self.mode = Some(ZoteroMode::Live);
        Ok(loaded)
    }
    fn publish_closed(
        &mut self,
        snapshot: &closed::ClosedSnapshot,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        let personal = snapshot
            .libraries
            .iter()
            .find(|library| library.group_id.is_none())
            .ok_or(ZoteroError::InvalidData)?;
        let warnings = snapshot
            .warnings
            .iter()
            .filter_map(|warning| match warning {
                closed::ClosedWarning::CloudSyncedDirectory => Some(WarningCode::CloudSyncedDirectory),
                closed::ClosedWarning::ItemTooLarge { library_id, .. } if *library_id == personal.library_id => {
                    Some(WarningCode::WorkLimit)
                }
                _ => None,
            })
            .map(|code| SourceWarning { code, line: None })
            .collect();
        let loaded = self.map_items(
            personal.items.iter().map(|item| {
                let raw = &item.data;
                ZoteroItem {
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
                }
            }),
            warnings,
            cancelled,
        )?;
        self.current = Some(loaded.clone());
        self.mode = Some(ZoteroMode::Closed);
        self.reader = LiveReader::new(Library::User);
        Ok(loaded)
    }
    fn map_items(
        &self,
        items: impl IntoIterator<Item = ZoteroItem>,
        mut warnings: Vec<SourceWarning>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        let library = library_id();
        let mut objects = Vec::new();
        for item in items {
            if cancelled() {
                return Err(LiveError::Cancelled.into());
            }
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
                Err(MappingError::InputLimit) => warnings.push(SourceWarning {
                    code: WarningCode::WorkLimit,
                    line: None,
                }),
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
        Ok(loaded)
    }
}

// Hold the existing private-root lease only while reading. An idle desktop
// must not keep the shared temporary cache locked against a separate CLI.
fn read_closed_at(
    data: &Path,
    cache: &Path,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<closed::ClosedSnapshot, ZoteroError> {
    let reader = closed::ClosedReader::new(cache)?;
    reader.read_cancellable(data, cancelled).map_err(Into::into)
}

fn default_directory() -> Result<PathBuf, ZoteroError> {
    let platform = if cfg!(target_os = "macos") {
        discovery::Platform::MacOs
    } else if cfg!(windows) {
        discovery::Platform::Windows
    } else if cfg!(target_os = "linux") {
        discovery::Platform::Linux
    } else {
        return Err(ZoteroError::Location);
    };
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok_or(ZoteroError::Location)?;
    let roots = discovery::DiscoveryRoots::new(platform, home.into(), std::env::var_os("APPDATA").map(PathBuf::from));
    let found = discovery::discover(&roots, None);
    if !found.warnings.is_empty() || found.directories.len() != 1 {
        return Err(ZoteroError::Location);
    }
    Ok(found.directories[0].path().to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use ibis_core::zotero::api::Response;
    use serde_json::json;
    struct ParityApi;
    impl Transport for ParityApi {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            let body = if path.ends_with("format=versions") {
                json!({"SYNTH001":1,"SYNTH002":1})
            } else {
                json!([{"key":"SYNTH001","version":1,"data":{"key":"SYNTH001","version":1,"itemType":"book",
                "title":"The Synthetic Atlas","date":"Spring 2020","accessDate":"2020-02-03 04:05:06","citationKey":"Synthetic2020",
                "creators":[{"creatorType":"author","firstName":"Ada","lastName":"Example"},{"creatorType":"editor","name":"Synthetic Institute","fieldMode":1}],
                "dateModified":"2020-02-02T00:00:00Z"}},
                {"key":"SYNTH002","version":1,"data":{"key":"SYNTH002","version":1,"itemType":"book","title":"x".repeat(300_000),"creators":[],"dateModified":"2020-02-02T00:00:00Z"}}])
            };
            Ok(Response {
                status: 200,
                body: serde_json::to_vec(&body).unwrap(),
                headers: vec![
                    ("Zotero-API-Version".into(), "3".into()),
                    ("X-Zotero-Version".into(), "10.0.6".into()),
                    ("Zotero-Server-ID".into(), "invented-parity".into()),
                    ("Last-Modified-Version".into(), "1".into()),
                    ("Total-Results".into(), "2".into()),
                ],
            })
        }
    }
    #[test]
    fn a_live_item_limit_publishes_other_records_with_a_fixed_warning() {
        let loaded = ZoteroSource::default()
            .refresh(&mut ParityApi, false, &|| false)
            .unwrap();
        assert_eq!(loaded.snapshot.objects().len(), 1);
        assert_eq!(
            loaded.warnings,
            vec![SourceWarning {
                code: WarningCode::WorkLimit,
                line: None
            }]
        );
    }
    #[test]
    fn mapper_item_limits_skip_the_item_without_discarding_other_records() {
        let normal = ZoteroItem {
            key: "SYNTH001".into(),
            item_type: "book".into(),
            ..ZoteroItem::default()
        };
        let mut large = normal.clone();
        large.key = "SYNTH002".into();
        large.fields.insert(
            "extra".into(),
            format!(
                "tex.ids: {}",
                (0..4097).map(|n| format!("Alias{n}")).collect::<Vec<_>>().join(",")
            ),
        );
        let loaded = ZoteroSource::default()
            .map_items([normal, large], vec![], &|| false)
            .unwrap();
        assert_eq!(loaded.snapshot.objects().len(), 1);
        assert_eq!(
            loaded.warnings,
            vec![SourceWarning {
                code: WarningCode::WorkLimit,
                line: None
            }]
        );
    }
    #[test]
    fn actual_closed_reader_matches_live_public_records_in_disposable_ci() {
        if !cfg!(target_os = "macos") || std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
            eprintln!("actual-reader parity requires disposable macOS CI; native local check skipped");
            return;
        }
        let data = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let db = rusqlite::Connection::open(data.path().join("zotero.sqlite")).unwrap();
        db.execute_batch(include_str!("../../ibis-zotero-local/fixtures/closed/schema.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../ibis-zotero-local/fixtures/closed/zotero10.sql"))
            .unwrap();
        db.execute(
            "INSERT INTO itemDataValues(valueID,value) VALUES(6,?1)",
            rusqlite::params!["x".repeat(300_000)],
        )
        .unwrap();
        db.execute_batch("INSERT INTO items VALUES(17,1,'SYNTH002',1,1,'2020-01-01 00:00:00','2020-02-02 00:00:00'); INSERT INTO itemData VALUES(17,1,6);").unwrap();
        drop(db);
        let before = std::fs::read(data.path().join("zotero.sqlite")).unwrap();
        let acquired = read_closed_at(data.path(), cache.path(), Arc::new(|| false)).unwrap();
        // A completed source load must release its cache lease for another
        // process/reader, rather than retaining it for the app's lifetime.
        closed::ClosedReader::new(cache.path())
            .unwrap()
            .read(data.path())
            .unwrap();
        let mut closed = ZoteroSource::default();
        let closed_data = closed.publish_closed(&acquired, &|| false).unwrap();
        let mut live = ZoteroSource::default();
        let live_data = live.refresh(&mut ParityApi, false, &|| false).unwrap();
        assert_eq!(closed.mode(), Some(ZoteroMode::Closed));
        assert_eq!(live.mode(), Some(ZoteroMode::Live));
        assert_eq!(
            closed.mode().unwrap().capabilities(),
            live.mode().unwrap().capabilities()
        );
        assert_eq!(closed_data.snapshot.objects().len(), 1);
        assert_eq!(
            serde_json::to_value(&closed_data.snapshot).unwrap(),
            serde_json::to_value(&live_data.snapshot).unwrap()
        );
        assert_eq!(closed_data.snapshot.fingerprints(), live_data.snapshot.fingerprints());
        assert_eq!(closed_data.warnings, live_data.warnings);
        assert_eq!(std::fs::read(data.path().join("zotero.sqlite")).unwrap(), before);
        assert!(!data.path().join("zotero.sqlite-shm").exists());
    }
}
