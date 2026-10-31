//! Raw live acquisition and immutable publication through the shared mapper.
use crate::{
    LoadedLibrary, SourceWarning, WarningCode,
    zotero::{self, MappingError, MappingWarning, ZoteroCreator, ZoteroItem},
};
use ibis_core::contract::{CitationKeySource, ItemRef, LibraryId, LibraryObject, Snapshot};
use ibis_core::zotero::api::{Transport, TransportError};
use ibis_core::zotero::libraries::{self, Group};
pub use ibis_zotero_local::DEFAULT_PORT;
use ibis_zotero_local::bbt::{BbtError, BbtLibrary, LiveBbt};
pub use ibis_zotero_local::live::{AttachmentKind, Library, LiveError};
use ibis_zotero_local::live::{Limits, LiveReader, LiveWarning};
use ibis_zotero_local::{closed, discovery};
use std::collections::BTreeMap;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use thiserror::Error;

/// The most libraries one choice may name.
pub const MAX_LIBRARIES: usize = 32;

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
    #[error("a chosen Zotero library was not found")]
    LibraryNotFound,
    #[error("the choice of Zotero libraries is invalid")]
    Selection,
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
fn live_error(error: LiveError) -> ZoteroError {
    if error == LiveError::GroupNotFound {
        ZoteroError::LibraryNotFound
    } else {
        error.into()
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
/// One Zotero library's opaque identity. Live and closed reads share it, so
/// the same item keeps its reference whether Zotero runs or not; group IDs are
/// Zotero's public group IDs, which both readers see.
pub fn library_id(library: Library) -> LibraryId {
    match library {
        Library::User => LibraryId::zotero(b"default:user"),
        Library::Group(id) => LibraryId::zotero(format!("default:group:{id}").as_bytes()),
    }
}
pub fn is_zotero(library: &LibraryId) -> bool {
    library.as_str().starts_with("zotero:")
}
#[derive(Clone)]
pub struct ZoteroSource {
    /// One reader per chosen library, in the chosen order.
    readers: Vec<LiveReader>,
    current: Vec<Arc<LoadedLibrary>>,
    mode: Option<ZoteroMode>,
}
impl Default for ZoteroSource {
    fn default() -> Self {
        Self {
            readers: Self::readers(&[Library::User]),
            current: vec![],
            mode: None,
        }
    }
}
impl ZoteroSource {
    /// A source that reads `libraries`, in that order. The personal library
    /// is read only when chosen; a group is never replaced by another library.
    pub fn new(libraries: &[Library]) -> Result<Self, ZoteroError> {
        let valid = |library: &Library| match library {
            Library::User => true,
            Library::Group(id) => *id != 0 && i64::try_from(*id).is_ok(),
        };
        let unique = libraries
            .iter()
            .enumerate()
            .all(|(n, library)| !libraries[..n].contains(library));
        if libraries.is_empty() || libraries.len() > MAX_LIBRARIES || !unique || !libraries.iter().all(valid) {
            return Err(ZoteroError::Selection);
        }
        Ok(Self {
            readers: Self::readers(libraries),
            current: vec![],
            mode: None,
        })
    }
    /// One refresh of the whole choice stays within one library's limits:
    /// each library gets an equal share of the requests, memory, items and time.
    fn readers(libraries: &[Library]) -> Vec<LiveReader> {
        let whole = Limits::default();
        let share = u32::try_from(libraries.len()).unwrap_or(u32::MAX).max(1);
        let limits = Limits {
            requests: whole.requests / share as usize,
            bytes: whole.bytes / share as usize,
            items: whole.items / share as usize,
            elapsed: whole.elapsed / share,
        };
        libraries
            .iter()
            .map(|library| LiveReader::with_limits(*library, limits))
            .collect()
    }
    pub fn libraries(&self) -> Vec<Library> {
        self.readers.iter().map(LiveReader::library).collect()
    }
    /// The last published libraries, in the chosen order.
    pub fn current(&self) -> &[Arc<LoadedLibrary>] {
        &self.current
    }
    pub fn mode(&self) -> Option<ZoteroMode> {
        self.mode
    }
    pub fn refresh_local(
        &mut self,
        port: u16,
        reload: bool,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Vec<Arc<LoadedLibrary>>, ZoteroError> {
        self.refresh_observed(port, reload, cancelled, &mut || Ok(()))
    }
    pub fn refresh_observed(
        &mut self,
        port: u16,
        reload: bool,
        cancelled: &impl Fn() -> bool,
        on_full: &mut impl FnMut() -> Result<(), LiveError>,
    ) -> Result<Vec<Arc<LoadedLibrary>>, ZoteroError> {
        self.refresh_live(
            &mut ibis_zotero_local::LocalZotero::on_port(port),
            Some(LiveBbt::on_port(port)),
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
    ) -> Result<Vec<Arc<LoadedLibrary>>, ZoteroError> {
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
    ) -> Result<Vec<Arc<LoadedLibrary>>, ZoteroError> {
        self.refresh_live(transport, None, reload, cancelled, &mut || Ok(()))
    }
    fn refresh_live(
        &mut self,
        transport: &mut impl Transport,
        bbt: Option<LiveBbt>,
        reload: bool,
        cancelled: &impl Fn() -> bool,
        on_full: &mut impl FnMut() -> Result<(), LiveError>,
    ) -> Result<Vec<Arc<LoadedLibrary>>, ZoteroError> {
        // Every chosen library is read before any is published, so a
        // failure keeps the last complete set.
        let mut readers = self.readers.clone();
        for reader in &mut readers {
            reader
                .refresh_observed(transport, reload, cancelled, on_full)
                .map_err(live_error)?;
        }
        let mut keys = BbtKeys {
            bbt,
            libraries: None,
            groups: None,
        };
        let mut loaded = Vec::with_capacity(readers.len());
        for reader in &readers {
            // Better BibTeX key changes leave Zotero's item versions alone, so
            // every refresh asks again for every item without its own key.
            let keyless: Vec<&str> = reader
                .items()
                .into_iter()
                .filter(|raw| raw.fields.get("citationKey").is_none_or(|key| key.trim().is_empty()))
                .map(|raw| raw.key.as_str())
                .collect();
            let mut warnings: Vec<_> = reader
                .warnings()
                .map(|warning| SourceWarning {
                    code: match warning {
                        LiveWarning::ItemTooLarge => WarningCode::WorkLimit,
                        LiveWarning::MalformedItem => WarningCode::Field,
                    },
                    line: None,
                })
                .collect();
            let bbt_keys = keys.find(transport, reader.library(), &keyless).unwrap_or_else(|()| {
                warnings.push(SourceWarning {
                    code: WarningCode::CitationKeysUnavailable,
                    line: None,
                });
                BTreeMap::new()
            });
            if cancelled() {
                return Err(LiveError::Cancelled.into());
            }
            let mut mapped = self.map_items(
                library_id(reader.library()),
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
                    better_bibtex_keys: bbt_keys.get(&raw.key).cloned().into_iter().collect(),
                }),
                warnings,
                cancelled,
            )?;
            let best_attachments = reader
                .items()
                .into_iter()
                .filter_map(|raw| {
                    let kind = raw.best_attachment?;
                    let id = ItemRef {
                        library: mapped.library.clone(),
                        key: raw.key.clone(),
                    };
                    mapped.snapshot.get(&id).map(|_| (id, kind))
                })
                .collect();
            Arc::make_mut(&mut mapped).best_attachments = best_attachments;
            loaded.push(mapped);
        }
        self.readers = readers;
        self.current = loaded.clone();
        self.mode = Some(ZoteroMode::Live);
        Ok(loaded)
    }
    fn publish_closed(
        &mut self,
        snapshot: &closed::ClosedSnapshot,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Vec<Arc<LoadedLibrary>>, ZoteroError> {
        let chosen = self.libraries();
        let mut loaded = Vec::with_capacity(chosen.len());
        for &library in &chosen {
            let group_id = match library {
                Library::User => None,
                Library::Group(id) => Some(i64::try_from(id).map_err(|_| ZoteroError::Selection)?),
            };
            let Some(found) = snapshot.libraries.iter().find(|found| found.group_id == group_id) else {
                // Every Zotero database has a personal library.
                return Err(if group_id.is_none() {
                    ZoteroError::InvalidData
                } else {
                    ZoteroError::LibraryNotFound
                });
            };
            let warnings = snapshot
                .warnings
                .iter()
                .filter_map(|warning| match warning {
                    closed::ClosedWarning::CloudSyncedDirectory => Some(WarningCode::CloudSyncedDirectory),
                    closed::ClosedWarning::ItemTooLarge { library_id, .. } if *library_id == found.library_id => {
                        Some(WarningCode::WorkLimit)
                    }
                    _ => None,
                })
                .chain(
                    snapshot
                        .better_bibtex_unknown
                        .then_some(WarningCode::CitationKeysUnavailable),
                )
                .map(|code| SourceWarning { code, line: None })
                .collect();
            loaded.push(self.map_items(
                library_id(library),
                found.items.iter().map(|item| {
                    let raw = &item.data;
                    ZoteroItem {
                        key: raw.key.clone(),
                        item_type: raw.item_type.clone(),
                        fields: raw
                            .fields
                            .iter()
                            .map(|(name, value)| (name.clone(), value.clone()))
                            .chain(std::iter::once(("dateAdded".into(), item.metadata.date_added.clone())))
                            .collect(),
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
                        better_bibtex_keys: raw.better_bibtex_keys.clone(),
                    }
                }),
                warnings,
                cancelled,
            )?);
        }
        self.current = loaded.clone();
        self.mode = Some(ZoteroMode::Closed);
        self.readers = Self::readers(&chosen);
        Ok(loaded)
    }
    fn map_items(
        &self,
        library: LibraryId,
        items: impl IntoIterator<Item = ZoteroItem>,
        mut warnings: Vec<SourceWarning>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        // Better BibTeX may hold a key that disagrees with an item's Extra
        // key. While its keys are unknown, an item without Zotero's own key
        // keeps no key at all, so key output stays refused for it.
        let keys_unknown = warnings
            .iter()
            .any(|warning| warning.code == WarningCode::CitationKeysUnavailable);
        let mut objects = Vec::new();
        for item in items {
            if cancelled() {
                return Err(LiveError::Cancelled.into());
            }
            match zotero::map_item(library.clone(), &item) {
                Ok(mut mapped) => {
                    if keys_unknown
                        && !mapped
                            .record
                            .citation_keys
                            .iter()
                            .any(|key| key.source == CitationKeySource::Zotero)
                    {
                        mapped.record.citation_keys.clear();
                    }
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
            best_attachments: Default::default(),
        });
        Ok(loaded)
    }
}

/// Better BibTeX's keys during one live refresh. Its JSON-RPC names a group
/// by Zotero's local library ID, which the local API never shows, so a group
/// is matched by name, asked once per refresh. Both names are compared as
/// Ibis shows them, and each must be unique on both sides: an ambiguous name
/// gives no keys rather than another library's.
struct BbtKeys {
    bbt: Option<LiveBbt>,
    libraries: Option<Result<Vec<BbtLibrary>, BbtError>>,
    groups: Option<Option<Vec<Group>>>,
}
impl BbtKeys {
    /// `Err` means Better BibTeX holds keys this refresh cannot read. Without
    /// Better BibTeX (Zotero answers 404) the items simply have no key.
    fn find(
        &mut self,
        transport: &mut impl Transport,
        library: Library,
        keyless: &[&str],
    ) -> Result<BTreeMap<String, String>, ()> {
        let Some(bbt) = self.bbt.filter(|_| !keyless.is_empty()) else {
            return Ok(BTreeMap::new());
        };
        let local = match library {
            Library::User => None,
            Library::Group(id) => {
                // An older Better BibTeX without `user.groups` is `Malformed`:
                // it holds group keys this refresh cannot place.
                let known = match self.libraries.get_or_insert_with(|| bbt.libraries()) {
                    Ok(known) => known,
                    Err(BbtError::Unavailable) => return Ok(BTreeMap::new()),
                    Err(_) => return Err(()),
                };
                let groups = self
                    .groups
                    .get_or_insert_with(|| libraries::groups(transport).ok())
                    .as_ref()
                    .ok_or(())?;
                let name = &groups.iter().find(|group| group.id == id).ok_or(())?.name;
                if groups.iter().filter(|group| group.name == *name).count() != 1 {
                    return Err(());
                }
                let mut same = known
                    .iter()
                    .filter(|library| libraries::clean_name(&library.name) == *name);
                match (same.next(), same.next()) {
                    (Some(library), None) => Some(library.library_id),
                    _ => return Err(()),
                }
            }
        };
        match bbt.keys(local, keyless) {
            Ok(found) => Ok(found),
            Err(BbtError::Unavailable) => Ok(BTreeMap::new()),
            Err(_) => Err(()),
        }
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
    use serde_json::{Value, json};
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
            .unwrap()
            .remove(0);
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
            .map_items(library_id(Library::User), [normal, large], vec![], &|| false)
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
    fn keys(loaded: &LoadedLibrary) -> Vec<(String, CitationKeySource)> {
        loaded
            .snapshot
            .objects()
            .iter()
            .flat_map(|object| match &object.object {
                LibraryObject::Record(record) => record.citation_keys.clone(),
                _ => vec![],
            })
            .map(|key| (key.value, key.source))
            .collect()
    }

    fn unavailable(loaded: &LoadedLibrary) -> bool {
        loaded
            .warnings
            .iter()
            .any(|warning| warning.code == WarningCode::CitationKeysUnavailable)
    }

    type Answer = Arc<std::sync::Mutex<(u16, Value)>>;

    /// Zotero's local API and Better BibTeX's JSON-RPC on one loopback port,
    /// as in Zotero. SYNTH001 has no key of its own and `extra` as Extra.
    /// `bbt` is the JSON-RPC answer's status and body, changeable between
    /// refreshes; the returned list collects the JSON-RPC requests.
    fn serve(extra: &'static str, bbt: Answer) -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let asked = Arc::new(std::sync::Mutex::new(vec![]));
        let seen = asked.clone();
        // Serves until the test process ends.
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = BufReader::new(stream.unwrap());
                let (mut head, mut line, mut length) = (String::new(), String::new(), 0);
                while stream.read_line(&mut line).unwrap() > 2 {
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                    head.push_str(&line);
                    line.clear();
                }
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                let (status, answer) = if head.starts_with("POST /better-bibtex/json-rpc ") {
                    seen.lock().unwrap().push(String::from_utf8(body).unwrap());
                    bbt.lock().unwrap().clone()
                } else if head.contains("format=versions") {
                    (200, json!({"SYNTH001":1,"SYNTH002":1}))
                } else {
                    (
                        200,
                        json!([
                            {"key":"SYNTH001","version":1,"data":{"key":"SYNTH001","version":1,"itemType":"book",
                            "title":"Invented atlas","extra":extra,"creators":[],"dateModified":"2026-10-26T12:00:00Z"}},
                            {"key":"SYNTH002","version":1,"data":{"key":"SYNTH002","version":1,"itemType":"book",
                            "title":"Invented field guide","citationKey":"Own2026","creators":[],"dateModified":"2026-10-26T12:00:00Z"}}
                        ]),
                    )
                };
                let answer = serde_json::to_vec(&answer).unwrap();
                let _ = write!(
                    stream.get_mut(),
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\nZotero-API-Version: 3\r\n\
                     X-Zotero-Version: 7.0.11\r\nZotero-Server-ID: invented-bbt\r\nLast-Modified-Version: 1\r\nTotal-Results: 2\r\n\r\n",
                    answer.len()
                );
                let _ = stream.get_mut().write_all(&answer);
            }
        });
        (port, asked)
    }

    #[test]
    fn live_reads_ask_better_bibtex_only_for_items_without_their_own_key() {
        let (port, server) = serve("", answer(200, json!({"SYNTH001":"Invented2026"})));
        let loaded = ZoteroSource::default()
            .refresh_local(port, false, &|| false)
            .unwrap()
            .remove(0);
        assert_eq!(
            *server.lock().unwrap(),
            vec![r#"{"id":1,"jsonrpc":"2.0","method":"item.citationkey","params":[["SYNTH001"]]}"#]
        );
        let mut found = keys(&loaded);
        found.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            found,
            vec![
                ("Invented2026".into(), CitationKeySource::BetterBibtex),
                ("Own2026".into(), CitationKeySource::Zotero),
            ]
        );
        assert!(!unavailable(&loaded));
    }

    #[test]
    fn live_better_bibtex_failures_publish_no_guessed_keys() {
        // Without Better BibTeX, Zotero answers 404: those items simply have
        // no key. Any other failure says the keys are unavailable.
        for (status, warned) in [(404, false), (500, true)] {
            let (port, server) = serve("", answer(status, json!({})));
            let loaded = ZoteroSource::default()
                .refresh_local(port, false, &|| false)
                .unwrap()
                .remove(0);
            assert_eq!(server.lock().unwrap().len(), 1);
            assert_eq!(keys(&loaded), vec![("Own2026".into(), CitationKeySource::Zotero)]);
            assert_eq!(unavailable(&loaded), warned, "{status}");
        }
    }

    fn answer(status: u16, result: Value) -> Answer {
        Arc::new(std::sync::Mutex::new((
            status,
            json!({"jsonrpc":"2.0","id":1,"result":result}),
        )))
    }

    #[test]
    fn a_better_bibtex_failure_cannot_clear_a_key_conflict() {
        let bbt = answer(200, json!({"SYNTH001":"Actual2026"}));
        let (port, _) = serve("Citation Key: Extra2026", bbt.clone());
        let mut source = ZoteroSource::default();
        let loaded = source.refresh_local(port, false, &|| false).unwrap().remove(0);
        let mut found = keys(&loaded);
        found.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            found,
            vec![
                ("Actual2026".into(), CitationKeySource::BetterBibtex),
                ("Extra2026".into(), CitationKeySource::Extra),
                ("Own2026".into(), CitationKeySource::Zotero),
            ]
        );
        // Better BibTeX fails on the next refresh, with Zotero unchanged.
        bbt.lock().unwrap().0 = 500;
        let loaded = source.refresh_local(port, false, &|| false).unwrap().remove(0);
        assert!(unavailable(&loaded));
        assert_eq!(keys(&loaded), vec![("Own2026".into(), CitationKeySource::Zotero)]);
        assert_eq!(loaded.snapshot.objects().len(), 2, "formatted output stays");
    }

    #[test]
    fn an_oversized_better_bibtex_key_keeps_the_reference() {
        let (port, _) = serve("", answer(200, json!({"SYNTH001":"k".repeat(257)})));
        let loaded = ZoteroSource::default()
            .refresh_local(port, false, &|| false)
            .unwrap()
            .remove(0);
        assert!(unavailable(&loaded));
        assert_eq!(loaded.snapshot.objects().len(), 2);
        assert_eq!(keys(&loaded), vec![("Own2026".into(), CitationKeySource::Zotero)]);
    }

    #[test]
    fn closed_reads_publish_better_bibtex_keys_and_say_when_they_are_unknown() {
        for unknown in [false, true] {
            let item = closed::ClosedItem {
                data: closed::RawItem {
                    key: "SYNTH001".into(),
                    item_type: "book".into(),
                    fields: [("title".into(), "Invented atlas".into())].into(),
                    better_bibtex_keys: vec!["Invented2026".into()],
                    ..Default::default()
                },
                metadata: closed::SourceMetadata {
                    item_id: 1,
                    library_id: 1,
                    version: 1,
                    date_added: "2026-10-26 12:00:00".into(),
                    date_modified: "2026-10-26 12:00:00".into(),
                    stored_fields: Default::default(),
                    stored_values: Default::default(),
                },
            };
            let snapshot = closed::ClosedSnapshot {
                schema_version: 123,
                libraries: vec![closed::ClosedLibrary {
                    library_id: 1,
                    group_id: None,
                    items: vec![item],
                }],
                warnings: vec![],
                better_bibtex_unknown: unknown,
            };
            let loaded = ZoteroSource::default()
                .publish_closed(&snapshot, &|| false)
                .unwrap()
                .remove(0);
            // Unknown keys leave an item without Zotero's own key keyless.
            let expected = if unknown {
                vec![]
            } else {
                vec![("Invented2026".into(), CitationKeySource::BetterBibtex)]
            };
            assert_eq!(keys(&loaded), expected);
            assert_eq!(unavailable(&loaded), unknown);
        }
    }

    /// Zotero with a personal library and group 7, both holding an item
    /// SYNTH001 without Zotero's own key, plus Better BibTeX. Group 7 is
    /// local library 2; `bbt_libraries` is what `user.groups` answers.
    /// Group 99 does not exist. Returns the port and the JSON-RPC requests.
    fn serve_groups(bbt_libraries: Value) -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
        serve_groups_with_answer(answer(200, bbt_libraries))
    }

    fn serve_groups_with_answer(bbt_libraries: Answer) -> (u16, Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let asked = Arc::new(std::sync::Mutex::new(vec![]));
        let seen = asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = BufReader::new(stream.unwrap());
                let (mut head, mut line, mut length) = (String::new(), String::new(), 0);
                while stream.read_line(&mut line).unwrap() > 2 {
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                    head.push_str(&line);
                    line.clear();
                }
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                let title = if head.contains("groups/7/") {
                    "Invented group atlas"
                } else {
                    "Invented atlas"
                };
                let (status, answer) = if head.starts_with("POST /better-bibtex/json-rpc ") {
                    let request: Value = serde_json::from_slice(&body).unwrap();
                    seen.lock().unwrap().push(request.to_string());
                    if request["method"] == "user.groups" {
                        bbt_libraries.lock().unwrap().clone()
                    } else {
                        let names = request["params"][0].as_array().unwrap();
                        let result: Value = names
                            .iter()
                            .map(|name| {
                                let name = name.as_str().unwrap();
                                let key = match name {
                                    "SYNTH001" => json!("Personal2026"),
                                    "2:SYNTH001" => json!("Group2026"),
                                    _ => Value::Null,
                                };
                                (name.to_owned(), key)
                            })
                            .collect::<serde_json::Map<_, _>>()
                            .into();
                        (200, json!({"jsonrpc":"2.0","id":1,"result":result}))
                    }
                } else if head.contains("groups/99/") {
                    (404, json!({}))
                } else if head.contains("users/0/groups") {
                    (
                        200,
                        json!([{"id":7,"version":1,"data":{"id":7,"name":"Invented group"}}]),
                    )
                } else if head.contains("format=versions") {
                    (200, json!({"SYNTH001":1}))
                } else {
                    (
                        200,
                        json!([{"key":"SYNTH001","version":1,"data":{"key":"SYNTH001","version":1,"itemType":"book",
                        "title":title,"creators":[],"dateModified":"2026-10-26T12:00:00Z"}}]),
                    )
                };
                let answer = serde_json::to_vec(&answer).unwrap();
                let _ = write!(
                    stream.get_mut(),
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\nZotero-API-Version: 3\r\n\
                     X-Zotero-Version: 7.0.11\r\nZotero-Server-ID: invented-groups\r\nLast-Modified-Version: 1\r\nTotal-Results: 1\r\n\r\n",
                    answer.len()
                );
                let _ = stream.get_mut().write_all(&answer);
            }
        });
        (port, asked)
    }

    fn invented_libraries() -> Value {
        json!([{"id":1,"name":"My Library"},{"id":2,"name":"Invented group"}])
    }

    #[test]
    fn chosen_libraries_publish_separately_with_their_own_identity_and_keys() {
        let (port, asked) = serve_groups(invented_libraries());
        let mut source = ZoteroSource::new(&[Library::User, Library::Group(7)]).unwrap();
        let loaded = source.refresh_local(port, false, &|| false).unwrap();
        assert_eq!(
            loaded.iter().map(|loaded| loaded.library.clone()).collect::<Vec<_>>(),
            vec![library_id(Library::User), library_id(Library::Group(7))]
        );
        assert_ne!(library_id(Library::Group(7)), library_id(Library::Group(8)));
        // The same item key in two libraries is two references.
        let ids: Vec<_> = loaded
            .iter()
            .flat_map(|loaded| loaded.snapshot.objects())
            .map(|object| object.object.id().clone())
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(
            keys(&loaded[0]),
            vec![("Personal2026".into(), CitationKeySource::BetterBibtex)]
        );
        assert_eq!(
            keys(&loaded[1]),
            vec![("Group2026".into(), CitationKeySource::BetterBibtex)]
        );
        assert!(!unavailable(&loaded[0]) && !unavailable(&loaded[1]));
        // Better BibTeX names the group by its local library ID.
        assert!(
            asked
                .lock()
                .unwrap()
                .iter()
                .any(|request| request.contains(r#"["2:SYNTH001"]"#))
        );
        assert_eq!(source.current().len(), 2);
    }

    #[test]
    fn unreadable_better_bibtex_libraries_warn_only_for_the_chosen_group() {
        for (status, reply) in [
            (500, json!({})),
            (200, json!({"jsonrpc":"2.0","id":1,"error":{"code":-32601}})),
            (200, json!({"jsonrpc":"2.0","id":1,"error":{"code":-32603}})),
            (200, json!({"jsonrpc":"2.0","id":1,"result":null})),
        ] {
            let (port, asked) = serve_groups_with_answer(Arc::new(std::sync::Mutex::new((status, reply))));
            for chosen in [[Library::User, Library::Group(7)], [Library::Group(7), Library::User]] {
                let loaded = ZoteroSource::new(&chosen)
                    .unwrap()
                    .refresh_local(port, false, &|| false)
                    .unwrap();
                for (library, loaded) in chosen.into_iter().zip(loaded) {
                    assert_eq!(loaded.snapshot.objects().len(), 1);
                    assert_eq!(unavailable(&loaded), library == Library::Group(7), "{status}");
                    assert_eq!(
                        keys(&loaded),
                        if library == Library::User {
                            vec![("Personal2026".into(), CitationKeySource::BetterBibtex)]
                        } else {
                            vec![]
                        }
                    );
                }
            }
            let asked = asked.lock().unwrap();
            assert_eq!(
                asked.iter().filter(|request| request.contains("user.groups")).count(),
                2
            );
            assert_eq!(
                asked
                    .iter()
                    .filter(|request| request.contains("item.citationkey"))
                    .count(),
                2
            );
        }
    }

    #[test]
    fn zotero_without_better_bibtex_gives_a_group_no_keys_and_no_warning() {
        // Zotero answers 404 for Better BibTeX's endpoint when it is absent.
        let (port, _) = serve_groups_with_answer(answer(404, json!({})));
        let loaded = ZoteroSource::new(&[Library::Group(7)])
            .unwrap()
            .refresh_local(port, false, &|| false)
            .unwrap();
        assert!(keys(&loaded[0]).is_empty());
        assert!(!unavailable(&loaded[0]));
    }

    #[test]
    fn an_ambiguous_group_name_leaves_only_that_group_without_keys() {
        let (port, _) = serve_groups(json!([
            {"id":1,"name":"My Library"},{"id":2,"name":"Invented group"},{"id":3,"name":"Invented group"}
        ]));
        let loaded = ZoteroSource::new(&[Library::User, Library::Group(7)])
            .unwrap()
            .refresh_local(port, false, &|| false)
            .unwrap();
        assert!(!unavailable(&loaded[0]));
        assert_eq!(keys(&loaded[0]).len(), 1);
        assert!(unavailable(&loaded[1]));
        assert_eq!(keys(&loaded[1]), vec![]);
        assert_eq!(loaded[1].snapshot.objects().len(), 1, "the reference stays citable");
    }

    #[test]
    fn group_names_are_compared_as_shown_and_must_be_unique() {
        // "Invented group " shows as Zotero's "Invented group": two Better
        // BibTeX libraries then share the name, so neither is guessed.
        let (port, asked) = serve_groups(json!([
            {"id":1,"name":"My Library"},{"id":2,"name":"Invented group"},{"id":3,"name":"Invented group\u{200B} "}
        ]));
        let loaded = ZoteroSource::new(&[Library::Group(7)])
            .unwrap()
            .refresh_local(port, false, &|| false)
            .unwrap();
        assert!(unavailable(&loaded[0]));
        assert!(
            !asked
                .lock()
                .unwrap()
                .iter()
                .any(|request| request.contains("item.citationkey"))
        );
        // A name Zotero's list shows cleaned still matches Better BibTeX's raw one.
        let (port, _) = serve_groups(json!([{"id":1,"name":"My Library"},{"id":2,"name":" Invented group\u{202E}"}]));
        let loaded = ZoteroSource::new(&[Library::Group(7)])
            .unwrap()
            .refresh_local(port, false, &|| false)
            .unwrap();
        assert_eq!(
            keys(&loaded[0]),
            vec![("Group2026".into(), CitationKeySource::BetterBibtex)]
        );
    }

    #[test]
    fn a_missing_group_fails_the_read_and_never_becomes_the_personal_library() {
        let (port, _) = serve_groups(invented_libraries());
        for chosen in [vec![Library::Group(99)], vec![Library::User, Library::Group(99)]] {
            let mut source = ZoteroSource::new(&chosen).unwrap();
            assert_eq!(
                source.refresh_local(port, false, &|| false).unwrap_err(),
                ZoteroError::LibraryNotFound
            );
            assert!(source.current().is_empty());
        }
    }

    #[test]
    fn invalid_choices_are_refused() {
        for chosen in [
            vec![],
            vec![Library::Group(0)],
            vec![Library::Group(u64::MAX)],
            vec![Library::User, Library::User],
            (1..=33).map(Library::Group).collect(),
        ] {
            assert_eq!(ZoteroSource::new(&chosen).err(), Some(ZoteroError::Selection));
        }
        assert_eq!(ZoteroSource::default().libraries(), vec![Library::User]);
    }

    fn closed_library(library_id: i64, group_id: Option<i64>, title: &str, bbt: &str) -> closed::ClosedLibrary {
        closed::ClosedLibrary {
            library_id,
            group_id,
            items: vec![closed::ClosedItem {
                data: closed::RawItem {
                    key: "SYNTH001".into(),
                    item_type: "book".into(),
                    fields: [("title".into(), title.into())].into(),
                    better_bibtex_keys: vec![bbt.into()],
                    ..Default::default()
                },
                metadata: closed::SourceMetadata {
                    item_id: library_id,
                    library_id,
                    version: 1,
                    date_added: "2026-10-26 12:00:00".into(),
                    date_modified: "2026-10-26 12:00:00".into(),
                    stored_fields: Default::default(),
                    stored_values: Default::default(),
                },
            }],
        }
    }

    #[test]
    fn closed_and_live_reads_of_chosen_libraries_publish_the_same_records() {
        let snapshot = closed::ClosedSnapshot {
            schema_version: 123,
            libraries: vec![
                closed_library(1, None, "Invented atlas", "Personal2026"),
                closed_library(2, Some(7), "Invented group atlas", "Group2026"),
                closed_library(3, Some(8), "Unchosen group", "Other2026"),
            ],
            warnings: vec![],
            better_bibtex_unknown: false,
        };
        let (port, _) = serve_groups(invented_libraries());
        for chosen in [
            vec![Library::Group(7)],
            vec![Library::User, Library::Group(7)],
            vec![Library::Group(7), Library::User],
        ] {
            let closed = ZoteroSource::new(&chosen)
                .unwrap()
                .publish_closed(&snapshot, &|| false)
                .unwrap();
            let live = ZoteroSource::new(&chosen)
                .unwrap()
                .refresh_local(port, false, &|| false)
                .unwrap();
            assert_eq!(closed.len(), chosen.len());
            for (closed, live) in closed.iter().zip(&live) {
                assert_eq!(closed.library, live.library);
                assert_eq!(
                    serde_json::to_value(&closed.snapshot).unwrap(),
                    serde_json::to_value(&live.snapshot).unwrap()
                );
                assert_eq!(closed.snapshot.fingerprints(), live.snapshot.fingerprints());
                assert_eq!(closed.warnings, live.warnings);
            }
        }
        let mut missing = ZoteroSource::new(&[Library::Group(99)]).unwrap();
        assert_eq!(
            missing.publish_closed(&snapshot, &|| false).unwrap_err(),
            ZoteroError::LibraryNotFound
        );
    }

    /// The local API serving `items`, each at version 1.
    struct ItemsApi(Vec<Value>);
    impl Transport for ItemsApi {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            let body = if path.ends_with("format=versions") {
                Value::Object(
                    self.0
                        .iter()
                        .map(|item| (item["key"].as_str().unwrap().to_owned(), json!(1)))
                        .collect(),
                )
            } else {
                Value::Array(
                    self.0
                        .iter()
                        .map(|item| {
                            let mut data = item.clone();
                            data["version"] = json!(1);
                            data["dateModified"] = json!("2020-02-02T00:00:00Z");
                            json!({"key": item["key"], "version": 1, "data": data})
                        })
                        .collect(),
                )
            };
            Ok(Response {
                status: 200,
                body: serde_json::to_vec(&body).unwrap(),
                headers: vec![
                    ("Zotero-API-Version".into(), "3".into()),
                    ("X-Zotero-Version".into(), "10.0.6".into()),
                    ("Zotero-Server-ID".into(), "invented-goldens".into()),
                    ("Last-Modified-Version".into(), "1".into()),
                    ("Total-Results".into(), self.0.len().to_string()),
                ],
            })
        }
    }

    /// Writes `items` into a synthetic Zotero 10 database the way Zotero stores
    /// them: one row per field value, dates in Zotero's multipart form and
    /// single-field names with `fieldMode` 1.
    fn write_database(db: &rusqlite::Connection, items: &[Value]) {
        db.execute_batch(include_str!("../../ibis-zotero-local/fixtures/closed/schema.sql"))
            .unwrap();
        db.execute_batch(
            "DELETE FROM items; DELETE FROM itemData; DELETE FROM itemDataValues; DELETE FROM creators;
             DELETE FROM itemCreators; DELETE FROM deletedItems; INSERT INTO version VALUES('userdata',130);",
        )
        .unwrap();
        let id = |db: &rusqlite::Connection, table: &str, key: &str, name: &str| -> i64 {
            let found = db.query_row(&format!("SELECT rowid FROM {table} WHERE {key}=?1"), [name], |row| {
                row.get(0)
            });
            found.unwrap_or_else(|_| {
                db.execute(&format!("INSERT INTO {table}({key}) VALUES(?1)"), [name])
                    .unwrap();
                db.last_insert_rowid()
            })
        };
        for (row, item) in (1_i64..).zip(items) {
            let kind = id(db, "itemTypesCombined", "typeName", item["itemType"].as_str().unwrap());
            db.execute(
                "INSERT INTO items VALUES(?1,1,?2,1,?3,'2020-01-01 00:00:00','2020-02-02 00:00:00')",
                rusqlite::params![row, item["key"].as_str().unwrap(), kind],
            )
            .unwrap();
            for (field, value) in item.as_object().unwrap() {
                let Some(value) = value.as_str() else { continue };
                if matches!(field.as_str(), "key" | "itemType") {
                    continue;
                }
                let stored = if field == "date" {
                    // Every golden date is a bare year.
                    assert!(value.len() == 4 && value.bytes().all(|b| b.is_ascii_digit()), "{value}");
                    format!("{value}-00-00 {value}")
                } else {
                    value.to_owned()
                };
                let field = id(db, "fieldsCombined", "fieldName", field);
                let value = id(db, "itemDataValues", "value", &stored);
                db.execute("INSERT INTO itemData VALUES(?1,?2,?3)", [row, field, value])
                    .unwrap();
            }
            for (order, creator) in (0_i64..).zip(item["creators"].as_array().unwrap()) {
                let (first, last, mode) = match creator["name"].as_str() {
                    Some(name) => ("", name, 1),
                    None => (
                        creator["firstName"].as_str().unwrap(),
                        creator["lastName"].as_str().unwrap(),
                        0,
                    ),
                };
                db.execute(
                    "INSERT INTO creators(firstName,lastName,fieldMode) VALUES(?1,?2,?3)",
                    rusqlite::params![first, last, mode],
                )
                .unwrap();
                let person = db.last_insert_rowid();
                let role = id(
                    db,
                    "creatorTypes",
                    "creatorType",
                    creator["creatorType"].as_str().unwrap(),
                );
                db.execute(
                    "INSERT INTO itemCreators VALUES(?1,?2,?3,?4)",
                    [row, person, role, order],
                )
                .unwrap();
            }
        }
    }

    /// The command-line goldens replay the local API's records of these items
    /// against pinned Zotero, so a closed read that publishes the same records
    /// formats the same way.
    #[test]
    fn golden_items_read_from_a_closed_database_publish_the_local_api_records() {
        if !cfg!(target_os = "macos") || std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
            eprintln!("actual-reader goldens require disposable macOS CI; native local check skipped");
            return;
        }
        let fixture: Value =
            serde_json::from_str(include_str!("../../../fixtures/formatted/zotero/expected.json")).unwrap();
        let items = fixture["items"].as_array().unwrap().clone();
        let data = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        write_database(
            &rusqlite::Connection::open(data.path().join("zotero.sqlite")).unwrap(),
            &items,
        );
        let acquired = read_closed_at(data.path(), cache.path(), Arc::new(|| false)).unwrap();
        let closed = ZoteroSource::default()
            .publish_closed(&acquired, &|| false)
            .unwrap()
            .remove(0);
        let live = ZoteroSource::default()
            .refresh(&mut ItemsApi(items.clone()), false, &|| false)
            .unwrap()
            .remove(0);
        assert_eq!(closed.snapshot.objects().len(), items.len());
        assert_eq!(
            serde_json::to_value(&closed.snapshot).unwrap(),
            serde_json::to_value(&live.snapshot).unwrap()
        );
        assert_eq!(closed.snapshot.fingerprints(), live.snapshot.fingerprints());
        assert_eq!(closed.warnings, live.warnings);
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
        let closed_data = closed.publish_closed(&acquired, &|| false).unwrap().remove(0);
        let mut live = ZoteroSource::default();
        let live_data = live.refresh(&mut ParityApi, false, &|| false).unwrap().remove(0);
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
