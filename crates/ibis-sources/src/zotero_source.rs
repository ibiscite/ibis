//! Raw live acquisition and immutable publication through the shared mapper.
use crate::{
    LoadedLibrary, SourceWarning, WarningCode,
    zotero::{self, MappingError, MappingWarning, ZoteroCreator, ZoteroItem},
};
use ibis_core::contract::{CitationKeySource, LibraryId, LibraryObject, Snapshot};
use ibis_core::zotero::api::{Transport, TransportError};
pub use ibis_zotero_local::DEFAULT_PORT;
use ibis_zotero_local::bbt::{BbtError, LiveBbt};
pub use ibis_zotero_local::live::LiveError;
use ibis_zotero_local::live::{Library, LiveReader};
use ibis_zotero_local::{closed, discovery};
use std::collections::BTreeMap;
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
        self.refresh_live(transport, None, reload, cancelled, &mut || Ok(()))
    }
    fn refresh_live(
        &mut self,
        transport: &mut impl Transport,
        bbt: Option<LiveBbt>,
        reload: bool,
        cancelled: &impl Fn() -> bool,
        on_full: &mut impl FnMut() -> Result<(), LiveError>,
    ) -> Result<Arc<LoadedLibrary>, ZoteroError> {
        let mut reader = self.reader.clone();
        reader.refresh_observed(transport, reload, cancelled, on_full)?;
        // Better BibTeX key changes leave Zotero's item versions alone, so
        // every refresh asks again for every item without its own key.
        let keyless: Vec<&str> = reader
            .items()
            .into_iter()
            .filter(|raw| raw.fields.get("citationKey").is_none_or(|key| key.trim().is_empty()))
            .map(|raw| raw.key.as_str())
            .collect();
        let mut warnings = vec![
            SourceWarning {
                code: WarningCode::WorkLimit,
                line: None
            };
            reader.omitted_items()
        ];
        let bbt_keys = match bbt.filter(|_| !keyless.is_empty()).map(|bbt| bbt.keys(None, &keyless)) {
            None | Some(Err(BbtError::Unavailable)) => BTreeMap::new(),
            Some(Ok(found)) => found,
            Some(Err(_)) => {
                warnings.push(SourceWarning {
                    code: WarningCode::CitationKeysUnavailable,
                    line: None,
                });
                BTreeMap::new()
            }
        };
        if cancelled() {
            return Err(LiveError::Cancelled.into());
        }
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
                better_bibtex_keys: bbt_keys.get(&raw.key).cloned().into_iter().collect(),
            }),
            warnings,
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
            .chain(
                snapshot
                    .better_bibtex_unknown
                    .then_some(WarningCode::CitationKeysUnavailable),
            )
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
                    better_bibtex_keys: raw.better_bibtex_keys.clone(),
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
        let loaded = ZoteroSource::default().refresh_local(port, false, &|| false).unwrap();
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
            let loaded = ZoteroSource::default().refresh_local(port, false, &|| false).unwrap();
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
        let loaded = source.refresh_local(port, false, &|| false).unwrap();
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
        let loaded = source.refresh_local(port, false, &|| false).unwrap();
        assert!(unavailable(&loaded));
        assert_eq!(keys(&loaded), vec![("Own2026".into(), CitationKeySource::Zotero)]);
        assert_eq!(loaded.snapshot.objects().len(), 2, "formatted output stays");
    }

    #[test]
    fn an_oversized_better_bibtex_key_keeps_the_reference() {
        let (port, _) = serve("", answer(200, json!({"SYNTH001":"k".repeat(257)})));
        let loaded = ZoteroSource::default().refresh_local(port, false, &|| false).unwrap();
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
            let loaded = ZoteroSource::default().publish_closed(&snapshot, &|| false).unwrap();
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
