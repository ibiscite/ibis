//! The source adapter protocol (decision 0056, step 4): how Ibis asks an
//! `ibis-source-<name>` program for one library.
//!
//! Ibis starts the program with no arguments and no environment, writes one
//! [`AdapterRequest`] as JSON to its standard input and closes it, then reads
//! one [`AdapterResponse`] from its standard output. The program exits with
//! status 0 whether it reads or refuses. Ibis keeps identity, validation, the
//! privacy filter and citation; the program only says what its manager holds.
//! The program's standard error is discarded.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The protocol version both sides speak; it changes only additively.
pub const ADAPTER_PROTOCOL: u32 = 1;

/// What Ibis asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterRequest {
    /// The protocol version, [`ADAPTER_PROTOCOL`].
    pub ibis: u32,
    pub method: AdapterMethod,
    /// Which library, in the adapter's own terms, exactly as the person
    /// named it. Omitted, the adapter reads its only library or refuses
    /// with `library-needed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterMethod {
    /// Read the whole library once.
    Read,
}

/// The adapter's one answer: exactly one of `read` and `refused`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterResponse {
    /// The protocol version, [`ADAPTER_PROTOCOL`].
    pub ibis: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<AdapterRead>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<AdapterRefusal>,
}

/// One whole-library read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterRead {
    /// Names the library whose record ids these are, at most 256
    /// characters. It must change whenever the adapter cannot show that an
    /// id still names the same record as before, for example when the
    /// library was replaced by a copy; Ibis then forgets every earlier
    /// handle. It is never shown.
    pub scope: String,
    /// Changes whenever any record changes, and only then; at most 256
    /// characters. `ibis sources check` holds the adapter to it; Ibis's own
    /// `revision` is still computed from what it reads.
    pub revision: String,
    /// Whether `citation-key` holds the manager's own citation keys. When
    /// false, Ibis drops any `citation-key`, and key outputs refuse with
    /// `missing-key`.
    #[serde(default)]
    pub keys: bool,
    /// CSL-JSON 1.0.2 records. Each `id` is the manager's own record id,
    /// unique and stable within `scope`. Ibis validates every field, drops
    /// `file` and private links, and skips records it cannot use, as it
    /// does for records sent to `cite --records`.
    #[schemars(with = "Vec<serde_json::Map<String, serde_json::Value>>")]
    pub records: Vec<serde_json::Value>,
    /// Records the adapter left out, so the person hears why.
    #[serde(default)]
    pub skipped: Vec<AdapterSkip>,
}

/// One record the adapter left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterSkip {
    pub cause: AdapterSkipCause,
    /// Its title, shown only in the person's library diagnostics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// The manager's name for the field at fault.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterSkipCause {
    /// A reference type the adapter does not map yet.
    UnsupportedType,
    /// A field every record needs, such as its title, is empty.
    MissingField,
    /// Values that disagree.
    Conflicting,
    /// The record could not be read.
    Unreadable,
}

/// Why the adapter did not read: fixed reasons, never the manager's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterRefusal {
    /// The manager must be open to be read, and it is not.
    ManagerClosed,
    /// Several libraries could be meant, or none was named; name one.
    LibraryNeeded,
    /// No library by that name is open or exists.
    LibraryNotFound,
    /// The operating system did not let the adapter ask the manager.
    PermissionDenied,
    /// The library could not be read.
    Unreadable,
    /// This adapter does not run on this system.
    Unsupported,
}
