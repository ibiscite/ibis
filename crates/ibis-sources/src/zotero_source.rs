//! Raw live acquisition and immutable publication through the shared mapper.
use crate::pdf::PdfReader;
use crate::reading::{DocumentText, SourceAttachment, TextState};
use crate::zotero_local::bbt::{BbtError, BbtLibrary, LiveBbt};
use crate::zotero_local::closed;
pub use crate::zotero_local::closed::ClosedError;
pub use crate::zotero_local::discovery;
use crate::zotero_local::fulltext;
pub use crate::zotero_local::live::{Library, LiveError};
use crate::zotero_local::live::{Limits, LiveReader, LiveWarning};
pub use crate::zotero_local::{DEFAULT_PORT, LocalZotero};
use crate::{
    LibraryKind, LoadedLibrary, SourceWarning, WarningCode,
    zotero::{self, MappingError, MappingWarning, ZoteroCreator, ZoteroItem},
};
use ibis_core::contract::ItemRef;
use ibis_core::contract::{CitationKeySource, ErrorReason, LibraryId, LibraryObject, Snapshot};
use ibis_core::zotero::api::{Transport, TransportError};
use ibis_core::zotero::libraries;
pub use ibis_core::zotero::libraries::Group;
use serde_json::Value;
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
    #[error("Zotero is closed and which profile to read is not settled; choose one")]
    ProfileNeeded,
    #[error("a chosen Zotero library was not found")]
    LibraryNotFound,
    #[error("the choice of Zotero libraries is invalid")]
    Selection,
    #[error("the attachment file is outside the library's folder; Settings › AI Apps can allow it")]
    AttachmentOutsideLibrary,
}
impl ZoteroError {
    pub fn cancelled(self) -> bool {
        matches!(
            self,
            Self::Live(LiveError::Cancelled) | Self::Closed(closed::ClosedError::Cancelled)
        )
    }
    pub fn newer(self) -> bool {
        matches!(self, Self::Closed(closed::ClosedError::TooNew { .. }))
    }
    pub fn schema_table(self) -> Option<ibis_core::contract::ZoteroTable> {
        match self {
            Self::Closed(closed::ClosedError::TooNew { table }) => Some(table),
            _ => None,
        }
    }
    pub fn limited(self) -> bool {
        matches!(
            self,
            Self::Closed(closed::ClosedError::TooLarge)
                | Self::Live(LiveError::Limit | LiveError::Transport(TransportError::TooLarge))
        )
    }
    /// What a person can do about this failure, when the kind of failure says.
    pub fn reason(self) -> Option<ErrorReason> {
        match self {
            Self::AttachmentOutsideLibrary => Some(ErrorReason::AttachmentOutsideLibrary),
            Self::Live(LiveError::NotAllowed) => Some(ErrorReason::ZoteroSharingOff),
            Self::ProfileNeeded => Some(ErrorReason::ZoteroProfileNeeded),
            Self::Closed(closed::ClosedError::NotFound) => Some(ErrorReason::ZoteroNotFound),
            Self::Closed(closed::ClosedError::NoSpace) => Some(ErrorReason::ZoteroNoSpace),
            Self::Location
            | Self::Closed(
                closed::ClosedError::Running
                | closed::ClosedError::ProcessCheck
                | closed::ClosedError::UnsafeSource
                | closed::ClosedError::Unreadable
                | closed::ClosedError::PrivateStorage
                | closed::ClosedError::LockPermissions
                | closed::ClosedError::Damaged
                | closed::ClosedError::TooOld
                | closed::ClosedError::NeedsRecovery,
            ) => Some(ErrorReason::ZoteroNeedsOpening),
            // Passing trouble: the same read may work, so nothing else is advised.
            _ => None,
        }
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
/// What tells one Zotero profile from another, so two profiles never share an
/// id. Live and closed reads of one profile must agree on it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Profile {
    /// The identity of the lone data directory, or one a caller fixed.
    Known(Vec<u8>),
    /// Several directories are configured. Zotero's API never says which
    /// directory it runs on, so both reads go by whose library the profile
    /// holds: a live read asks the running Zotero, a closed read the chosen
    /// profile's database, and the two name the owner the same way.
    Running,
}

/// One Zotero library's opaque identity: its profile plus the library. Live and
/// closed reads share it, so the same item keeps its reference whether Zotero
/// runs or not; group IDs are Zotero's public group IDs, which both readers see.
fn library_id(profile: &[u8], library: Library) -> LibraryId {
    crate::identity::library_id(LibraryKind::Zotero, &[profile, library.to_string().as_bytes()])
}
#[derive(Clone)]
pub struct ZoteroSource {
    /// One reader per chosen library, in the chosen order.
    readers: Vec<LiveReader>,
    profile: Profile,
    current: Vec<Arc<LoadedLibrary>>,
    mode: Option<ZoteroMode>,
    /// The profile to read while Zotero is closed, by its safe name.
    profile_name: Option<String>,
    /// Groups the last closed read listed; Zotero has no API while closed.
    closed_groups: Vec<Group>,
    /// What the last closed read found of attachments; nothing after a live
    /// read, which asks Zotero instead.
    closed_attachments: Option<Arc<ClosedAttachments>>,
    /// The data directory whose storage folder holds attachment files and
    /// Zotero's text cache: the one a closed read copied, or while Zotero runs
    /// the lone or chosen directory found on this computer.
    data: Option<PathBuf>,
}

/// Attachments by library and parent key, from one closed read.
type ClosedAttachments = BTreeMap<LibraryId, BTreeMap<String, Vec<closed::ClosedAttachment>>>;
impl Default for ZoteroSource {
    fn default() -> Self {
        Self {
            readers: Self::readers(&[Library::User]),
            profile: Profile::Known(vec![]),
            current: vec![],
            mode: None,
            profile_name: None,
            closed_groups: vec![],
            closed_attachments: None,
            data: None,
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
            profile: Profile::Known(vec![]),
            current: vec![],
            mode: None,
            profile_name: None,
            closed_groups: vec![],
            closed_attachments: None,
            data: None,
        })
    }
    /// Read this profile while Zotero is closed. A running Zotero answers for
    /// whichever profile it opened, so this says nothing about a live read.
    pub fn with_profile(mut self, profile: Option<String>) -> Self {
        self.profile_name = profile;
        self
    }
    pub fn profile(&self) -> Option<&str> {
        self.profile_name.as_deref()
    }
    /// Group libraries the last read listed, when that read was closed.
    pub fn closed_groups(&self) -> Option<&[Group]> {
        (self.mode == Some(ZoteroMode::Closed)).then_some(&self.closed_groups)
    }
    /// Read as this profile from now on, whatever discovery would say.
    pub fn on_profile(mut self, identity: &[u8]) -> Self {
        self.profile = Profile::Known(identity.to_vec());
        self
    }
    /// Resolve live/closed identity from read-only, caller-supplied discovery.
    pub fn use_discovery(&mut self, found: &discovery::Discovery) {
        self.profile = read_profile(Ok(found));
    }
    /// The same profile, reading a new choice of libraries.
    pub fn choosing(&self, libraries: &[Library]) -> Result<Self, ZoteroError> {
        let mut chosen = Self::new(libraries)?;
        chosen.profile = self.profile.clone();
        chosen.profile_name = self.profile_name.clone();
        Ok(chosen)
    }
    /// The id this source gives `library`, for a known profile.
    pub fn id_of(&self, library: Library) -> Option<LibraryId> {
        match &self.profile {
            Profile::Known(profile) => Some(library_id(profile, library)),
            Profile::Running => None,
        }
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
            &mut crate::zotero_local::LocalZotero::on_port(port),
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
        // Discovery reads two small configuration files and no library.
        let found = here();
        // A chosen profile only says which database a closed read copies;
        // it never changes the identity either read gives.
        match &found {
            Ok(found) => self.use_discovery(found),
            Err(_) => self.profile = Profile::Running,
        }
        // Zotero's API never says where its files are; while it runs they are
        // looked for only in the directory a closed read would copy.
        self.data = found
            .as_ref()
            .ok()
            .and_then(|found| chosen_directory(found, self.profile_name.as_deref()).ok())
            .map(|data| closed::trusted_folder(&data));
        match self.refresh_observed(DEFAULT_PORT, reload, &|| cancelled(), on_full) {
            Ok(loaded) => return Ok(loaded),
            Err(ZoteroError::Live(LiveError::Transport(TransportError::Unreachable))) => (),
            Err(error) => return Err(error),
        }
        on_full()?;
        if cancelled() {
            return Err(LiveError::Cancelled.into());
        }
        let data = found.and_then(|found| chosen_directory(&found, self.profile_name.as_deref()))?;
        let cache = crate::identity::cache_dir().ok_or(closed::ClosedError::PrivateStorage)?;
        let acquired = read_closed_at(&data, &cache, cancelled.clone())
            .map_err(|error| chosen_read_failed(error, self.profile_name.as_deref()))?;
        self.publish_closed(&acquired, Some(&data), &|| cancelled())
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
        let profile = match &self.profile {
            Profile::Known(profile) => profile.clone(),
            Profile::Running => running_owner(transport)?,
        };
        // Every chosen library is read before any is published, so a
        // failure keeps the last complete set.
        let mut readers = self.readers.clone();
        for reader in &mut readers {
            reader
                .refresh_observed(transport, reload, cancelled, on_full)
                .map_err(live_error)?;
        }
        // The same owner on both sides of the read: the items are that owner's.
        if self.profile == Profile::Running && running_owner(transport)? != profile {
            return Err(LiveError::Changing.into());
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
                    key: None,
                    entry: None,
                    cause: None,
                    field: None,
                })
                .collect();
            let bbt_keys = keys.find(transport, reader.library(), &keyless).unwrap_or_else(|()| {
                warnings.push(SourceWarning {
                    code: WarningCode::CitationKeysUnavailable,
                    line: None,
                    key: None,
                    entry: None,
                    cause: None,
                    field: None,
                });
                BTreeMap::new()
            });
            if cancelled() {
                return Err(LiveError::Cancelled.into());
            }
            let mapped = self.map_items(
                library_id(&profile, reader.library()),
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
            loaded.push(mapped);
        }
        self.readers = readers;
        self.current = loaded.clone();
        self.mode = Some(ZoteroMode::Live);
        self.closed_attachments = None;
        Ok(loaded)
    }
    fn publish_closed(
        &mut self,
        snapshot: &closed::ClosedSnapshot,
        data: Option<&Path>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Vec<Arc<LoadedLibrary>>, ZoteroError> {
        let mut attachments = BTreeMap::new();
        let chosen = self.libraries();
        let profile = match &self.profile {
            Profile::Known(profile) => profile.clone(),
            Profile::Running => owner_identity(snapshot.owner),
        };
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
                    closed::ClosedWarning::NewerThanTested => Some(WarningCode::ZoteroNewerThanTested),
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
                .map(|code| SourceWarning {
                    code,
                    line: None,
                    key: None,
                    entry: None,
                    cause: None,
                    field: None,
                })
                .collect();
            if let Some(found_attachments) = &snapshot.attachments {
                let mut by_parent = BTreeMap::<String, Vec<closed::ClosedAttachment>>::new();
                for attachment in found_attachments
                    .iter()
                    .filter(|attachment| attachment.library_id == found.library_id)
                {
                    by_parent
                        .entry(attachment.parent.clone())
                        .or_default()
                        .push(attachment.clone());
                }
                attachments.insert(library_id(&profile, library), by_parent);
            }
            loaded.push(self.map_items(
                library_id(&profile, library),
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
        self.closed_attachments = snapshot.attachments.as_ref().map(|_| Arc::new(attachments));
        self.data = data.map(closed::trusted_folder);
        self.readers = Self::readers(&chosen);
        self.closed_groups = closed_groups(snapshot);
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
        let mut parsed = crate::ParsedLibrary::default();
        let candidates = crate::candidates::Candidates::collect(items, |item| (vec![(item.key.clone(), None)], item));
        for item in candidates.finish(&mut parsed.duplicate_keys, &mut parsed.warnings) {
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
                    let label = mapped
                        .record
                        .citation_keys
                        .first()
                        .map(|key| key.value.clone())
                        .or_else(|| {
                            mapped
                                .record
                                .csl
                                .get("title")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_owned)
                        });
                    warnings.extend(mapped.warnings.into_iter().map(|warning| {
                        let (code, cause, field) = match warning {
                            MappingWarning::UnknownField => {
                                (WarningCode::Field, crate::WarningCause::UnknownField, None)
                            }
                            MappingWarning::SkippedCreator => {
                                (WarningCode::Field, crate::WarningCause::Names, Some("creators"))
                            }
                            MappingWarning::InvalidCitationKey => {
                                (WarningCode::Field, crate::WarningCause::Value, Some("citationKey"))
                            }
                            MappingWarning::InvalidAlias => {
                                (WarningCode::Field, crate::WarningCause::Value, Some("extra"))
                            }
                            MappingWarning::PrivateUrl => (WarningCode::Field, crate::WarningCause::Link, Some("url")),
                            MappingWarning::DateParseLimit(field) => {
                                (WarningCode::WorkLimit, crate::WarningCause::TooLarge, Some(field))
                            }
                        };
                        SourceWarning::at(code, None)
                            .because(cause, field)
                            .for_entry(label.as_deref())
                    }));
                    parsed.records.push(mapped.record);
                }
                Err(MappingError::UnsupportedType) => warnings.push(
                    SourceWarning::at(WarningCode::Field, None)
                        .because(crate::WarningCause::UnknownType, Some("itemType"))
                        .for_entry(
                            item.fields
                                .get("citationKey")
                                .or_else(|| item.fields.get("title"))
                                .map(String::as_str),
                        ),
                ),
                Err(MappingError::InputLimit) => warnings.push(
                    SourceWarning::at(WarningCode::WorkLimit, None).for_entry(
                        item.fields
                            .get("citationKey")
                            .or_else(|| item.fields.get("title"))
                            .map(String::as_str),
                    ),
                ),
                Err(_) => return Err(ZoteroError::InvalidData),
            }
        }
        warnings.extend(parsed.warnings);
        let snapshot = Snapshot::new(parsed.records.into_iter().map(LibraryObject::Record).collect())
            .map_err(|_| ZoteroError::InvalidData)?;
        if cancelled() {
            return Err(LiveError::Cancelled.into());
        }
        let loaded = Arc::new(LoadedLibrary {
            library,
            kind: LibraryKind::Zotero,
            capabilities: crate::SourceCapability::ZOTERO,
            snapshot,
            warnings,
            duplicate_keys: parsed.duplicate_keys,
            cut_short: false,
            damaged_keys: Default::default(),
        });
        Ok(loaded)
    }
}

impl ZoteroSource {
    /// What reading attachments needs, taken out of the source so that no lock
    /// is held while Zotero answers or a file is read.
    pub fn reading(&self) -> ZoteroReading {
        ZoteroReading {
            libraries: self
                .current
                .iter()
                .map(|loaded| loaded.library.clone())
                .zip(self.libraries())
                .collect(),
            mode: self.mode,
            closed: self.closed_attachments.clone(),
            data: self.data.clone(),
        }
    }
}

/// The attachments of the published Zotero libraries and their text (decision
/// 0055): Zotero's local API while it runs, the last closed read and Zotero's
/// text cache while it is closed, and the PDF itself through the extractor's
/// child process when Zotero's index is missing or partial. Nothing is written.
#[derive(Clone)]
pub struct ZoteroReading {
    libraries: Vec<(LibraryId, Library)>,
    mode: Option<ZoteroMode>,
    closed: Option<Arc<ClosedAttachments>>,
    data: Option<PathBuf>,
}
impl ZoteroReading {
    fn library(&self, id: &LibraryId) -> Result<Library, ZoteroError> {
        self.libraries
            .iter()
            .find(|(library, _)| library == id)
            .map(|(_, library)| *library)
            .ok_or(ZoteroError::LibraryNotFound)
    }

    fn closed(&self, parent: &ItemRef) -> Result<&[closed::ClosedAttachment], ZoteroError> {
        let found = self
            .closed
            .as_ref()
            .ok_or(ZoteroError::Closed(closed::ClosedError::Unreadable))?;
        Ok(found
            .get(&parent.library)
            .and_then(|parents| parents.get(&parent.key))
            .map_or(&[], Vec::as_slice))
    }

    /// The attachments of `parent`, a record this source published.
    pub fn attachments(
        &self,
        transport: &mut impl Transport,
        parent: &ItemRef,
    ) -> Result<Vec<SourceAttachment>, ZoteroError> {
        let library = self.library(&parent.library)?;
        Ok(match self.mode {
            Some(ZoteroMode::Live) => fulltext::attachments(transport, library, &parent.key)
                .map_err(text_error)?
                .into_iter()
                .map(|found| SourceAttachment {
                    revision: found.revision,
                    key: found.key,
                    title: found.title,
                    media_type: found.content_type,
                    link_only: found.link_only,
                    location: found.path,
                })
                .collect(),
            Some(ZoteroMode::Closed) => self
                .closed(parent)?
                .iter()
                .take(fulltext::MAX_ATTACHMENTS)
                .map(|found| SourceAttachment {
                    revision: found.revision.clone(),
                    key: found.key.clone(),
                    title: found.title.clone(),
                    media_type: found.content_type.clone(),
                    link_only: found.link_only,
                    location: found.path.clone(),
                })
                .collect(),
            None => return Err(ZoteroError::LibraryNotFound),
        })
    }

    /// The text of `attachment`, one of `parent`'s attachments; `pdf` reads
    /// a PDF whose index is missing or partial, and its page labels.
    pub fn text(
        &self,
        transport: &mut impl Transport,
        parent: &ItemRef,
        attachment: &SourceAttachment,
        pdf: Option<&PdfReader>,
        outside_library: bool,
        cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<TextState, ZoteroError> {
        if cancelled() {
            return Err(ZoteroError::Closed(closed::ClosedError::Cancelled));
        }
        if attachment.link_only {
            return Ok(TextState::NoFile);
        }
        // Zotero base-folder links cannot establish storage containment here.
        // An empty location still permits indexed-only imported attachments.
        if !outside_library
            && !attachment.location.is_empty()
            && !attachment.location.starts_with("storage:")
            && !Path::new(&attachment.location).is_absolute()
        {
            return Err(ZoteroError::AttachmentOutsideLibrary);
        }
        let file = self.file(attachment, outside_library)?;
        if file.is_none() && Path::new(&attachment.location).is_absolute() {
            return Ok(TextState::Refused(crate::pdf::Refusal::Missing));
        }
        let library = self.library(&parent.library)?;
        let indexed = match self.mode {
            Some(ZoteroMode::Live) => fulltext::indexed(transport, library, &attachment.key)
                .map_err(text_error)?
                .map(|found| (found.text, found.indexed_pages, found.total_pages, Some(found.revision))),
            Some(ZoteroMode::Closed) => {
                let counts = self
                    .closed(parent)?
                    .iter()
                    .find(|found| found.key == attachment.key)
                    .and_then(|found| found.indexed);
                match (counts, self.data.as_deref()) {
                    (Some(counts), Some(data)) => closed::read_text_cache(data, &attachment.key, cancelled.clone())?
                        .map(|text| (text, counts.indexed_pages, counts.total_pages, None)),
                    _ => None,
                }
            }
            None => return Err(ZoteroError::LibraryNotFound),
        };
        let indexed = indexed.map(|(text, indexed_pages, total_pages, revision)| {
            let mut text = DocumentText::from_zotero(text, indexed_pages, total_pages);
            if let Some(revision) = revision {
                text.revision = revision;
            }
            text
        });
        if cancelled() {
            return Err(ZoteroError::Closed(closed::ClosedError::Cancelled));
        }
        let file = pdf.filter(|_| attachment.is_pdf()).zip(file);
        let text = crate::reading::text_of(
            indexed,
            file.as_ref().map(|(reader, file)| (*reader, file.as_path())),
            cancelled.clone(),
        );
        if cancelled() {
            return Err(ZoteroError::Closed(closed::ClosedError::Cancelled));
        }
        Ok(text)
    }

    /// The attachment's stored file or an absolute linked file. Relative linked
    /// files require Zotero's base-folder preference, which this source lacks.
    fn file(&self, attachment: &SourceAttachment, outside_library: bool) -> Result<Option<PathBuf>, ZoteroError> {
        let path = if let Some(name) = attachment.location.strip_prefix("storage:") {
            let mut parts = Path::new(name).components();
            let (Some(std::path::Component::Normal(name)), None) = (parts.next(), parts.next()) else {
                return Ok(None);
            };
            if !fulltext::valid_key(&attachment.key) {
                return Ok(None);
            }
            let Some(data) = &self.data else { return Ok(None) };
            data.join("storage").join(&attachment.key).join(name)
        } else {
            let path = Path::new(&attachment.location);
            if !path.is_absolute() {
                return Ok(None);
            }
            path.to_owned()
        };
        let base = self.data.as_ref().map(|data| data.join("storage"));
        if !outside_library && base.is_none() {
            return Err(ZoteroError::AttachmentOutsideLibrary);
        }
        crate::file::attachment_path(base.as_deref().unwrap_or(Path::new("")), &path, outside_library).map_err(
            |error| match error {
                crate::SourceError::AttachmentOutsideLibrary | crate::SourceError::Missing => {
                    ZoteroError::AttachmentOutsideLibrary
                }
                _ => ZoteroError::Location,
            },
        )
    }
}

fn text_error(error: fulltext::TextError) -> ZoteroError {
    match error {
        fulltext::TextError::Transport(transport) => LiveError::Transport(transport).into(),
        fulltext::TextError::TooLarge => LiveError::Limit.into(),
        fulltext::TextError::Malformed => ZoteroError::InvalidData,
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
// must not keep the per-user cache locked against a separate CLI.
fn read_closed_at(
    data: &Path,
    cache: &Path,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<closed::ClosedSnapshot, ZoteroError> {
    let reader = closed::ClosedReader::new(cache);
    reader.read_cancellable(data, cancelled).map_err(Into::into)
}

/// The groups a closed copy lists, named as the live list names them.
fn closed_groups(snapshot: &closed::ClosedSnapshot) -> Vec<Group> {
    snapshot
        .libraries
        .iter()
        .filter_map(|library| {
            let id = u64::try_from(library.group_id?).ok()?;
            let name = libraries::clean_name(&library.name);
            Some(Group {
                id,
                name: if name.is_empty() { format!("Group {id}") } else { name },
            })
        })
        .collect()
}

/// The profiles on this computer, by name; reads no library.
pub fn profiles() -> Result<Vec<String>, ZoteroError> {
    profile_names(&here()?)
}

/// Whether a Zotero library is on this computer, Zotero running or not. Reads
/// Zotero's profile configuration and checks that a database file exists; no
/// library is read before a person chooses Zotero.
#[allow(clippy::disallowed_methods)]
pub fn installed() -> bool {
    here().is_ok_and(|found| {
        found
            .directories
            .iter()
            .any(|found| found.path().join("zotero.sqlite").is_file())
    })
}

/// The groups a running Zotero lists; reads no library.
pub fn live_groups(port: u16) -> Result<Vec<Group>, ZoteroError> {
    libraries::groups(&mut crate::zotero_local::LocalZotero::on_port(port)).map_err(|error| match error {
        ibis_core::zotero::api::ZoteroError::NotAllowed => LiveError::NotAllowed.into(),
        ibis_core::zotero::api::ZoteroError::Transport(transport) => LiveError::Transport(transport).into(),
        _ => LiveError::Incomplete.into(),
    })
}

fn profile_names(found: &discovery::Discovery) -> Result<Vec<String>, ZoteroError> {
    if found.directories.is_empty() {
        return Err(ZoteroError::Location);
    }
    Ok(found.directories.iter().map(|found| found.name().to_owned()).collect())
}

/// The directory to read closed. A profile asked for by name must still be
/// there: like a missing group, it never falls back to another library.
fn chosen_directory(found: &discovery::Discovery, profile: Option<&str>) -> Result<PathBuf, ZoteroError> {
    if let Some(profile) = profile {
        return found
            .directories
            .iter()
            .find(|found| found.name() == profile)
            .map(|found| found.path().to_owned())
            .ok_or(ZoteroError::ProfileNeeded);
    }
    match found.directories.as_slice() {
        [only] if found.warnings.is_empty() => Ok(only.path().to_owned()),
        [_, _, ..] => Err(ZoteroError::ProfileNeeded),
        _ => Err(ZoteroError::Location),
    }
}

/// The profile both reads go by. Zotero's API never says which directory it
/// runs on, so only a lone directory names one; with several, live and closed
/// reads both go by the profile's owner.
fn read_profile(found: Result<&discovery::Discovery, ZoteroError>) -> Profile {
    found
        .and_then(|found| chosen_directory(found, None))
        .map_or(Profile::Running, |data| Profile::Known(directory_identity(&data)))
}

/// A chosen profile on an unplugged drive asks again; otherwise the choice
/// could never be changed from the screen that reports it.
fn chosen_read_failed(error: ZoteroError, profile: Option<&str>) -> ZoteroError {
    if profile.is_some() && error == ZoteroError::Closed(closed::ClosedError::NotFound) {
        ZoteroError::ProfileNeeded
    } else {
        error
    }
}

/// A directory's spelling from discovery, which both read paths share.
fn directory_identity(data: &Path) -> Vec<u8> {
    let mut identity = b"directory:".to_vec();
    let platform = if cfg!(windows) {
        discovery::Platform::Windows
    } else if cfg!(target_os = "macos") {
        discovery::Platform::MacOs
    } else {
        discovery::Platform::Linux
    };
    identity.extend_from_slice(&discovery::path_identity(data, platform));
    identity
}

/// Who owns the running profile's library, from any one item. Two profiles of
/// one Zotero user hold the same synced items, so sharing ids is right for them.
/// A never-synced profile has owner 0 and an empty one shows none; those can't
/// be told apart while Zotero runs, and item keys are random, so a reference
/// from one then finds nothing in the other.
fn running_owner(transport: &mut impl Transport) -> Result<Vec<u8>, ZoteroError> {
    let response = transport
        .get("users/0/items/top?limit=1&format=json")
        .map_err(LiveError::Transport)?;
    if response.status == 403 {
        return Err(LiveError::NotAllowed.into());
    }
    if response.status != 200 {
        return Err(LiveError::Status(response.status).into());
    }
    let rows: Value = serde_json::from_slice(&response.body).map_err(|_| ZoteroError::InvalidData)?;
    let owner = rows.get(0).and_then(|row| row.pointer("/library/id")?.as_u64());
    Ok(owner_identity(owner.unwrap_or_default()))
}

/// The identity of the profile whose personal library `owner` holds, as a live
/// read finds it and a closed read records it.
fn owner_identity(owner: u64) -> Vec<u8> {
    format!("owner:{owner}").into_bytes()
}

fn here() -> Result<discovery::Discovery, ZoteroError> {
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
    Ok(discovery::discover(&roots))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::zotero::api::Response;
    use serde_json::{Value, json};

    #[test]
    fn field_warnings_name_the_citation_key_or_title_and_the_date_field() {
        for (field, expected) in [
            ("date", "issued"),
            ("accessDate", "accessed"),
            ("originalDate", "original-date"),
        ] {
            for key in [None, Some("vale2024")] {
                let mut item = ZoteroItem {
                    key: "SYNTH001".into(),
                    item_type: "book".into(),
                    fields: BTreeMap::from([
                        ("title".into(), "Invented Lantern".into()),
                        (field.into(), "x".repeat(5000)),
                    ]),
                    ..ZoteroItem::default()
                };
                if let Some(key) = key {
                    item.fields.insert("citationKey".into(), key.into());
                }
                let loaded = ZoteroSource::default()
                    .map_items(LibraryId::digest(b"invented"), [item], vec![], &|| false)
                    .unwrap();
                let warning = loaded
                    .warnings
                    .iter()
                    .find(|warning| warning.field.as_deref() == Some(expected))
                    .unwrap();
                assert_eq!(warning.entry.as_deref(), Some(key.unwrap_or("Invented Lantern")));
                let LibraryObject::Record(record) = &loaded.snapshot.objects()[0].object else {
                    panic!("reference")
                };
                assert_eq!(record.csl[expected], json!({"literal":"x".repeat(5000)}));
            }
        }
    }
    fn registry(profiles: &[(&str, &str)]) -> (tempfile::TempDir, discovery::Discovery) {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".zotero/zotero");
        let mut ini = String::new();
        for (n, (name, directory)) in profiles.iter().enumerate() {
            let folder = root.join(format!("p{n}"));
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                folder.join("prefs.js"),
                format!(
                    "user_pref(\"extensions.zotero.dataDir\", \"{directory}\");\nuser_pref(\"extensions.zotero.useDataDir\", true);\n"
                ),
            )
            .unwrap();
            ini.push_str(&format!("[Profile{n}]\nName={name}\nIsRelative=1\nPath=p{n}\n"));
        }
        std::fs::write(root.join("profiles.ini"), ini).unwrap();
        let roots = discovery::DiscoveryRoots::new(discovery::Platform::Linux, home.path().into(), None);
        let found = discovery::discover(&roots);
        (home, found)
    }
    #[test]
    fn a_profiles_ini_with_several_profiles_asks_and_a_choice_reads_that_one() {
        let (_home, found) = registry(&[("Home", "/invented/home"), ("Work", "/invented/work")]);
        assert_eq!(profile_names(&found).unwrap(), ["Home", "Work"]);
        assert_eq!(chosen_directory(&found, None), Err(ZoteroError::ProfileNeeded));
        assert_eq!(
            ZoteroError::ProfileNeeded.reason(),
            Some(ErrorReason::ZoteroProfileNeeded)
        );
        assert_eq!(
            chosen_directory(&found, Some("Work")).unwrap(),
            Path::new("/invented/work")
        );
    }
    #[test]
    fn a_chosen_profile_that_is_gone_never_falls_back_to_another() {
        let (_home, found) = registry(&[("Home", "/invented/home")]);
        assert_eq!(chosen_directory(&found, None).unwrap(), Path::new("/invented/home"));
        assert_eq!(chosen_directory(&found, Some("Work")), Err(ZoteroError::ProfileNeeded));
    }
    #[test]
    fn each_failure_says_what_a_person_can_do() {
        assert_eq!(
            ZoteroError::Live(LiveError::NotAllowed).reason(),
            Some(ErrorReason::ZoteroSharingOff)
        );
        for fix_by_opening in [
            closed::ClosedError::Running,
            closed::ClosedError::ProcessCheck,
            closed::ClosedError::UnsafeSource,
            closed::ClosedError::Unreadable,
            closed::ClosedError::PrivateStorage,
            closed::ClosedError::LockPermissions,
            closed::ClosedError::Damaged,
            closed::ClosedError::TooOld,
            closed::ClosedError::NeedsRecovery,
        ] {
            assert_eq!(
                ZoteroError::Closed(fix_by_opening).reason(),
                Some(ErrorReason::ZoteroNeedsOpening)
            );
        }
        assert_eq!(ZoteroError::Location.reason(), Some(ErrorReason::ZoteroNeedsOpening));
        assert_eq!(
            ZoteroError::Closed(closed::ClosedError::NotFound).reason(),
            Some(ErrorReason::ZoteroNotFound)
        );
        assert_eq!(
            ZoteroError::Closed(closed::ClosedError::NoSpace).reason(),
            Some(ErrorReason::ZoteroNoSpace)
        );
        for no_advice in [
            ZoteroError::Closed(closed::ClosedError::TooNew {
                table: closed::ZoteroTable::Items,
            }),
            ZoteroError::Closed(closed::ClosedError::TooLarge),
            ZoteroError::Closed(closed::ClosedError::Cancelled),
            ZoteroError::Closed(closed::ClosedError::Changing),
            ZoteroError::Closed(closed::ClosedError::StorageInUse),
            ZoteroError::Closed(closed::ClosedError::TimedOut),
            ZoteroError::LibraryNotFound,
        ] {
            assert_eq!(no_advice.reason(), None);
        }
    }
    struct ParityApi;
    impl Transport for ParityApi {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            let body = if path.ends_with("format=versions") {
                json!({"SYNTH001":1,"SYNTH002":1})
            } else {
                json!([{"key":"SYNTH001","version":1,"data":{"key":"SYNTH001","version":1,"itemType":"book",
                "title":"The Synthetic Atlas","date":"Spring 2020","accessDate":"2020-02-03 04:05:06","citationKey":"Synthetic2020",
                "creators":[{"creatorType":"author","firstName":"Ada","lastName":"Example"},{"creatorType":"editor","name":"Synthetic Institute","fieldMode":1}],
                "dateAdded":"2020-01-01T00:00:00Z","dateModified":"2020-02-02T00:00:00Z"}},
                {"key":"SYNTH002","version":1,"data":{"key":"SYNTH002","version":1,"itemType":"book","title":"x".repeat(300_000),"creators":[],"dateAdded":"2020-01-01T00:00:00Z","dateModified":"2020-02-02T00:00:00Z"}}])
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
                line: None,
                key: None,
                entry: None,
                cause: None,
                field: None,
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
                    name: String::new(),
                    items: vec![item],
                }],
                warnings: vec![],
                better_bibtex_unknown: unknown,
                owner: 0,
                attachments: None,
            };
            let loaded = ZoteroSource::default()
                .publish_closed(&snapshot, None, &|| false)
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
                        "title":title,"creators":[],"dateAdded":"2026-10-26T12:00:00Z","dateModified":"2026-10-26T12:00:00Z"}}]),
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
    fn a_profile_is_its_data_directory_and_several_go_by_the_running_owner() {
        struct Owner(Option<u64>);
        impl Transport for Owner {
            fn get(&mut self, path: &str) -> Result<Response, TransportError> {
                // The real transport refuses a path with a leading slash.
                assert!(path.starts_with("users/0/items/top?"), "{path}");
                Ok(Response {
                    status: 200,
                    headers: vec![],
                    body: self
                        .0
                        .map_or(json!([]), |id| json!([{"library":{"id":id}}]))
                        .to_string()
                        .into_bytes(),
                })
            }
        }
        let identity = |path: &str| directory_identity(Path::new(path));
        assert_eq!(identity("/invented/one"), identity("/invented/one"));
        assert_ne!(
            library_id(&identity("/invented/one"), Library::User),
            library_id(&identity("/invented/two"), Library::User)
        );
        assert_ne!(
            library_id(&identity("/invented/one"), Library::User),
            library_id(&identity("/invented/one"), Library::Group(7))
        );
        // A group stays apart across profiles, though its ID is public.
        assert_ne!(
            library_id(&identity("/invented/one"), Library::Group(7)),
            library_id(&identity("/invented/two"), Library::Group(7))
        );
        assert_ne!(running_owner(&mut Owner(Some(4))), running_owner(&mut Owner(Some(5))));
        assert_eq!(running_owner(&mut Owner(None)), running_owner(&mut Owner(Some(0))));
    }

    #[test]
    fn a_change_of_owner_during_a_read_publishes_nothing() {
        struct Switching(Vec<u64>);
        impl Transport for Switching {
            fn get(&mut self, path: &str) -> Result<Response, TransportError> {
                let (headers, body) = if path.contains("limit=1") {
                    (vec![], json!([{"library":{"id":self.0.remove(0)}}]))
                } else {
                    let headers = [
                        ("Zotero-API-Version", "3"),
                        ("X-Zotero-Version", "10.0.6"),
                        ("Zotero-Server-ID", "invented"),
                        ("Last-Modified-Version", "1"),
                        ("Total-Results", "0"),
                    ];
                    (
                        headers.map(|(name, value)| (name.into(), value.into())).to_vec(),
                        json!({}),
                    )
                };
                Ok(Response {
                    status: 200,
                    headers,
                    body: body.to_string().into_bytes(),
                })
            }
        }
        let read = |owners: Vec<u64>| {
            let mut source = ZoteroSource {
                profile: Profile::Running,
                ..ZoteroSource::default()
            };
            source
                .refresh(&mut Switching(owners), false, &|| false)
                .map(|loaded| loaded.len())
        };
        assert_eq!(read(vec![4, 4]), Ok(1));
        assert_eq!(read(vec![4, 5]), Err(ZoteroError::Live(LiveError::Changing)));
    }

    #[test]
    fn several_data_directories_give_an_item_one_id_live_and_closed() {
        /// A running Zotero names its owner in each item it lists.
        struct Owned(u64, ItemsApi);
        impl Transport for Owned {
            fn get(&mut self, path: &str) -> Result<Response, TransportError> {
                if path.starts_with("users/0/items/top?limit=1&") {
                    return Ok(Response {
                        status: 200,
                        headers: vec![],
                        body: json!([{"library":{"id":self.0}}]).to_string().into_bytes(),
                    });
                }
                self.1.get(path)
            }
        }
        let chosen = [Library::User, Library::Group(7)];
        let several = || ZoteroSource {
            profile: Profile::Running,
            ..ZoteroSource::new(&chosen).unwrap()
        };
        let (_home, found) = registry(&[("Home", "/invented/home"), ("Work", "/invented/work")]);
        assert_eq!(read_profile(Ok(&found)), Profile::Running);
        let (_home, found) = registry(&[("Home", "/invented/home")]);
        assert_eq!(
            read_profile(Ok(&found)),
            Profile::Known(directory_identity(Path::new("/invented/home")))
        );
        assert_eq!(several().id_of(Library::User), None);
        let ids = |loaded: Vec<Arc<LoadedLibrary>>| {
            loaded
                .iter()
                .flat_map(|loaded| {
                    loaded
                        .snapshot
                        .objects()
                        .iter()
                        .map(|object| object.object.id().clone())
                })
                .collect::<Vec<_>>()
        };
        let live = |owner| {
            let item = json!({"key":"SYNTH001","itemType":"book","title":"Invented atlas","creators":[]});
            ids(several()
                .refresh(&mut Owned(owner, ItemsApi(vec![item])), false, &|| false)
                .unwrap())
        };
        let closed = |owner| {
            let snapshot = closed::ClosedSnapshot {
                schema_version: 123,
                libraries: vec![
                    closed_library(1, None, "Invented atlas", ""),
                    closed_library(2, Some(7), "Invented atlas", ""),
                ],
                warnings: vec![],
                better_bibtex_unknown: false,
                owner,
                attachments: None,
            };
            let mut source = several();
            let loaded = source.publish_closed(&snapshot, None, &|| false).unwrap();
            assert_eq!(source.mode(), Some(ZoteroMode::Closed));
            ids(loaded)
        };
        // Opening or closing Zotero keeps every reference, groups included.
        assert_eq!(closed(4242).len(), 2);
        assert_eq!(closed(4242), live(4242));
        assert_eq!(closed(0), live(0));
        // Another account's profile is another library.
        assert_ne!(closed(4242), closed(4243));
        assert_ne!(closed(4242)[1], closed(4243)[1]);
    }

    #[test]
    fn chosen_libraries_publish_separately_with_their_own_identity_and_keys() {
        let (port, asked) = serve_groups(invented_libraries());
        let mut source = ZoteroSource::new(&[Library::User, Library::Group(7)]).unwrap();
        let loaded = source.refresh_local(port, false, &|| false).unwrap();
        assert_eq!(
            loaded.iter().map(|loaded| loaded.library.clone()).collect::<Vec<_>>(),
            vec![library_id(b"", Library::User), library_id(b"", Library::Group(7))]
        );
        assert_ne!(library_id(b"", Library::Group(7)), library_id(b"", Library::Group(8)));
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

    #[test]
    fn newer_compatible_zotero_warns_on_the_library_and_not_its_records() {
        let snapshot = closed::ClosedSnapshot {
            schema_version: 132,
            libraries: vec![closed_library(1, None, "Invented atlas", "")],
            warnings: vec![closed::ClosedWarning::NewerThanTested],
            better_bibtex_unknown: false,
            owner: 0,
            attachments: None,
        };
        let loaded = ZoteroSource::default()
            .publish_closed(&snapshot, None, &|| false)
            .unwrap()
            .remove(0);
        assert_eq!(
            loaded.warnings,
            vec![SourceWarning::at(WarningCode::ZoteroNewerThanTested, None)]
        );
        let LibraryObject::Record(record) = &loaded.snapshot.objects()[0].object else {
            panic!("invented record");
        };
        assert!(record.warnings.is_empty());
    }

    fn closed_library(library_id: i64, group_id: Option<i64>, title: &str, bbt: &str) -> closed::ClosedLibrary {
        closed::ClosedLibrary {
            library_id,
            group_id,
            name: String::new(),
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
    fn a_closed_read_lists_attachments_and_reads_zoteros_text_cache() {
        let attachment = |key: &str, library_id, indexed: Option<u32>| closed::ClosedAttachment {
            revision: ibis_core::contract::Fingerprint::of_bytes(b"invented attachment revision"),
            library_id,
            parent: "SYNTH001".into(),
            key: key.into(),
            title: format!("Invented {key}"),
            content_type: "application/pdf".into(),
            link_only: false,
            path: format!("storage:{key}.pdf"),
            indexed: indexed.map(|pages| closed::IndexedCounts {
                indexed_pages: Some(pages),
                total_pages: Some(40),
            }),
        };
        let snapshot = closed::ClosedSnapshot {
            schema_version: 123,
            libraries: vec![
                closed_library(1, None, "Invented atlas", ""),
                closed_library(2, Some(7), "Invented group atlas", ""),
            ],
            warnings: vec![],
            better_bibtex_unknown: false,
            owner: 0,
            attachments: Some(vec![
                attachment("PDF00001", 1, Some(2)),
                attachment("PDF00002", 1, Some(3)),
                attachment("PDF00003", 1, None),
                attachment("GROUP001", 2, Some(1)),
            ]),
        };
        let data = tempfile::tempdir().unwrap();
        for key in ["PDF00001", "PDF00002"] {
            let folder = data.path().join("storage").join(key);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join(format!("{key}.pdf")), b"%PDF-1.7\n").unwrap();
            std::fs::write(
                folder.join(".zotero-ft-cache"),
                "Invented first page.\n\n\u{c}Invented second page.",
            )
            .unwrap();
        }
        let mut source = ZoteroSource::new(&[Library::User]).unwrap();
        let loaded = source.publish_closed(&snapshot, Some(data.path()), &|| false).unwrap();
        let parent = ItemRef {
            library: loaded[0].library.clone(),
            key: "SYNTH001".into(),
        };
        let reading = source.reading();
        let unused = &mut LocalZotero::on_port(0);
        let found = reading.attachments(unused, &parent).unwrap();
        // Only the chosen library's own attachments, never the group's.
        assert_eq!(
            found.iter().map(|found| found.key.as_str()).collect::<Vec<_>>(),
            ["PDF00001", "PDF00002", "PDF00003"]
        );
        // A stored file is looked for only in its own storage folder.
        assert_eq!(
            reading.file(&found[0], false).unwrap(),
            Some(closed::trusted_folder(data.path()).join("storage/PDF00001/PDF00001.pdf"))
        );
        let mut elsewhere = found[0].clone();
        for location in ["storage:../PDF00002/x.pdf", "storage:"] {
            elsewhere.location = location.into();
            assert_eq!(reading.file(&elsewhere, false).unwrap(), None, "{location}");
        }
        for location in ["attachments:papers/x.pdf", "papers/x.pdf"] {
            elsewhere.location = location.into();
            assert_eq!(
                reading
                    .text(unused, &parent, &elsewhere, None, false, Arc::new(|| false))
                    .err(),
                Some(ZoteroError::AttachmentOutsideLibrary)
            );
            assert!(matches!(
                reading.text(unused, &parent, &elsewhere, None, true, Arc::new(|| false)),
                Ok(TextState::Text(_))
            ));
        }
        // Absolute linked files obey the same boundary, before indexed text
        // is consulted; enabling this device's permission makes it readable.
        let linked = data.path().join("linked.pdf");
        std::fs::write(&linked, b"%PDF-1.7\n").unwrap();
        elsewhere.location = linked.to_string_lossy().into_owned();
        assert_eq!(
            reading
                .text(unused, &parent, &elsewhere, None, false, Arc::new(|| false))
                .err(),
            Some(ZoteroError::AttachmentOutsideLibrary)
        );
        assert!(matches!(
            reading.text(unused, &parent, &elsewhere, None, true, Arc::new(|| false)),
            Ok(TextState::Text(_))
        ));
        // Absolute files inside storage are allowed without that permission.
        elsewhere.location = data
            .path()
            .join("storage/PDF00001/PDF00001.pdf")
            .to_string_lossy()
            .into_owned();
        assert!(matches!(
            reading.text(unused, &parent, &elsewhere, None, false, Arc::new(|| false)),
            Ok(TextState::Text(_))
        ));
        #[cfg(unix)]
        {
            let link = data.path().join("storage/escape.pdf");
            std::os::unix::fs::symlink(&linked, &link).unwrap();
            elsewhere.location = link.to_string_lossy().into_owned();
            assert_eq!(
                reading
                    .text(unused, &parent, &elsewhere, None, false, Arc::new(|| false))
                    .err(),
                Some(ZoteroError::AttachmentOutsideLibrary)
            );
        }
        let TextState::Text(text) = reading
            .text(unused, &parent, &found[0], None, false, Arc::new(|| false))
            .unwrap()
        else {
            panic!("indexed text")
        };
        // A partial index cannot establish the document's page numbers.
        assert!(text.body == crate::reading::Body::Whole("Invented first page.\n\n\u{c}Invented second page.".into()));
        assert_eq!((text.indexed_pages, text.total_pages), (Some(2), Some(40)));
        // Zotero counted three pages: the breaks do not match, so pages are unknown.
        let TextState::Text(text) = reading
            .text(unused, &parent, &found[1], None, false, Arc::new(|| false))
            .unwrap()
        else {
            panic!("indexed text")
        };
        assert!(matches!(text.body, crate::reading::Body::Whole(_)));
        assert!(
            reading
                .text(unused, &parent, &found[2], None, false, Arc::new(|| false))
                .unwrap()
                == TextState::NotIndexed
        );
        let elsewhere = ItemRef {
            library: LibraryId::digest(b"invented other library"),
            key: "SYNTH001".into(),
        };
        assert_eq!(
            reading.attachments(unused, &elsewhere).err(),
            Some(ZoteroError::LibraryNotFound)
        );
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
            owner: 0,
            attachments: None,
        };
        let (port, _) = serve_groups(invented_libraries());
        for chosen in [
            vec![Library::Group(7)],
            vec![Library::User, Library::Group(7)],
            vec![Library::Group(7), Library::User],
        ] {
            let closed = ZoteroSource::new(&chosen)
                .unwrap()
                .publish_closed(&snapshot, None, &|| false)
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
            missing.publish_closed(&snapshot, None, &|| false).unwrap_err(),
            ZoteroError::LibraryNotFound
        );
    }

    /// The local API serving `items`, each at version 1, with the same creation
    /// metadata as `write_database` below.
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
                            data["dateAdded"] = json!("2020-01-01T00:00:00Z");
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
        db.execute_batch(include_str!("../fixtures/zotero/closed/schema.sql"))
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
            .publish_closed(&acquired, None, &|| false)
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
    #[allow(clippy::disallowed_methods)]
    fn actual_closed_reader_matches_live_public_records_in_disposable_ci() {
        if !cfg!(target_os = "macos") || std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
            eprintln!("actual-reader parity requires disposable macOS CI; native local check skipped");
            return;
        }
        let data = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let db = rusqlite::Connection::open(data.path().join("zotero.sqlite")).unwrap();
        db.execute_batch(include_str!("../fixtures/zotero/closed/schema.sql"))
            .unwrap();
        db.execute_batch(include_str!("../fixtures/zotero/closed/zotero10.sql"))
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
        closed::ClosedReader::new(cache.path()).read(data.path()).unwrap();
        let mut closed = ZoteroSource::default();
        let closed_data = closed.publish_closed(&acquired, None, &|| false).unwrap().remove(0);
        let mut live = ZoteroSource::default();
        let live_data = live.refresh(&mut ParityApi, false, &|| false).unwrap().remove(0);
        assert_eq!(closed.mode(), Some(ZoteroMode::Closed));
        assert_eq!(live.mode(), Some(ZoteroMode::Live));
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
