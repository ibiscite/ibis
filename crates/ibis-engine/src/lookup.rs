//! Online lookup, the only network client in Ibis (decisions 0008 and 0051).
//!
//! A DOI an open library already holds is cited from that library and never
//! sent. Otherwise, and only while the person has turned online lookup on and
//! agreed to the first DOI lookup after being told what is sent where, the DOI
//! alone goes to `doi.org`, which hands it to Crossref or DataCite. One request
//! runs at a time, within one deadline and a 1 MiB answer; no redirect is
//! followed except one checked `Location` from `doi.org`, and no proxy, cookie
//! or personal detail is used. The answer is untrusted: only its CSL variables
//! are kept, checked as records sent with a request are. It is in no library;
//! citing it goes through the same records path as `cite --records`.

use crate::health::SourceFinding;
use crate::settings::{self, SettingsError};
use crate::{Cancellation, Engine, SearchResult, request::SearchRequest};
use ibis_core::citable::DoiLink;
use ibis_core::contract::{EngineError, ErrorCode, ErrorReason, LibraryId, LookupService, Origin};
use ibis_core::identifier::{self, Identifier};
use ibis_core::search::MatchField;
use serde::Serialize;
use serde_json::{Map, Value};
use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The largest answer Ibis reads; anything longer is refused unread.
pub const MAX_ANSWER_BYTES: u64 = 1024 * 1024;
/// Both requests of one lookup share this deadline.
const DEADLINE: Duration = Duration::from_secs(10);
const CONNECT: Duration = Duration::from_secs(5);
const CSL_JSON: &str = "application/vnd.citationstyles.csl+json";
/// A generic client name: no person, computer or system detail.
const USER_AGENT: &str = concat!("Ibis/", env!("CARGO_PKG_VERSION"));
/// The resolver every DOI lookup starts at.
pub const RESOLVER: &str = "doi.org";

/// One request at a time, across every caller in this process.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// What a DOI lookup sends and where, as the person is told before the first one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Disclosure {
    /// The DOI, the only thing sent.
    pub doi: String,
    /// The host it is sent to.
    pub to: &'static str,
    /// The services that host may hand it to.
    pub then: [LookupService; 2],
}

/// Where the DOI was found.
#[derive(Debug)]
pub enum Found {
    /// In the person's open libraries: these hits, and nothing was sent.
    InLibrary(SearchResult),
    /// From a service; in none of the person's libraries.
    LookedUp(LookedUp),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LookedUp {
    pub origin: Origin,
    /// One checked CSL-JSON record with `id` `lookup`, ready for the records path.
    pub record: Value,
}

/// How the person answers before the first DOI lookup: `Some(true)` to send,
/// `Some(false)` to refuse, `None` when this run cannot ask anyone.
pub type Ask<'a> = &'a mut dyn FnMut(&Disclosure) -> Option<bool>;

/// The lookup client. It holds the only HTTP client Ibis has.
pub struct Lookup {
    transport: Box<dyn Transport>,
}

impl Default for Lookup {
    fn default() -> Self {
        Self::new()
    }
}

impl Lookup {
    pub fn new() -> Self {
        Self {
            transport: Box::new(Https::new(true)),
        }
    }

    /// Find `query`, a DOI in any spelling Ibis recognizes, in `engine`'s open
    /// libraries; failing that, look it up if `settings` allow it.
    pub fn doi(
        &self,
        engine: &Engine,
        settings: &settings::Owner,
        query: &str,
        ask: Ask<'_>,
        cancel: &Cancellation,
    ) -> Result<Found, EngineError> {
        let Some(Identifier::Doi(doi)) = identifier::parse_query(query) else {
            return Err(refusal(ErrorCode::BadInput, ErrorReason::NotADoi));
        };
        let link = DoiLink::new(&doi).ok_or_else(|| refusal(ErrorCode::BadInput, ErrorReason::NotADoi))?;
        if let Some(found) = in_library(engine, &doi, cancel)? {
            return Ok(Found::InLibrary(found));
        }
        let device = settings.read().cites().map_err(settings_error)?.device.clone();
        if device.online_lookup != Some(true) {
            return Err(refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupOff));
        }
        if !device.doi_lookup_confirmed {
            let disclosure = Disclosure {
                doi: doi.clone(),
                to: RESOLVER,
                then: LookupService::ALL,
            };
            match ask(&disclosure) {
                None => return Err(refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupUnconfirmed)),
                Some(false) => return Err(refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupDeclined)),
                Some(true) => {
                    // Saved before anything is sent; a lookup switched off meanwhile stays off.
                    let (_, on) = settings
                        .change(|settings| {
                            let on = settings.device.online_lookup == Some(true);
                            settings.device.doi_lookup_confirmed |= on;
                            Ok::<_, SettingsError>(on)
                        })
                        .map_err(|problem| settings_error(&problem))?;
                    if !on {
                        return Err(refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupOff));
                    }
                }
            }
        }
        cancel.check()?;
        let (service, answer) = self.fetch(&link)?;
        cancel.check()?;
        let record = checked(&answer, &doi)?;
        Ok(Found::LookedUp(LookedUp {
            origin: Origin::Lookup { doi, service },
            record,
        }))
    }

    /// The two requests: `doi.org`, then the one service its `Location` names.
    fn fetch(&self, link: &DoiLink) -> Result<(LookupService, Vec<u8>), EngineError> {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let start = Instant::now();
        let remaining = || {
            DEADLINE
                .checked_sub(start.elapsed())
                .filter(|left| !left.is_zero())
                .ok_or_else(|| refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupTimeout))
        };
        let first = self.transport.get(link.as_str(), remaining()?).map_err(failed)?;
        if !matches!(first.status, 301 | 302 | 303 | 307 | 308) {
            // `doi.org` never answers with a record itself.
            return Err(status(first.status));
        }
        let (service, next) = accepted(first.location.as_deref())?;
        drop(first);
        let mut answer = self.transport.get(next.as_str(), remaining()?).map_err(failed)?;
        if answer.status != 200 {
            return Err(status(answer.status));
        }
        let csl = answer.content_type.as_deref().is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case(CSL_JSON))
        });
        if !csl {
            return Err(malformed());
        }
        // Content-Length is not trusted: one byte past the limit is enough to refuse.
        let mut body = Vec::new();
        answer
            .body
            .by_ref()
            .take(MAX_ANSWER_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|error| failed(Failure::from(error)))?;
        if body.len() as u64 > MAX_ANSWER_BYTES {
            return Err(refusal(ErrorCode::ResourceLimit, ErrorReason::LookupTooLarge));
        }
        Ok((service, body))
    }
}

/// Every hit whose DOI is exactly this one, in every open library, each read
/// as it is now.
fn in_library(engine: &Engine, doi: &str, cancel: &Cancellation) -> Result<Option<SearchResult>, EngineError> {
    let mut result = engine.search(
        &SearchRequest {
            query: doi.to_owned(),
            limit: 1000,
            libraries: None,
            interpret_locator: false,
        },
        cancel,
    )?;
    result.hits.retain(|hit| hit.matched.contains(&MatchField::Doi));
    if !result.hits.is_empty() {
        return Ok(Some(result));
    }
    // A library read from an older copy may hold the DOI by now: never send it then.
    for finding in &result.source_health.findings {
        match finding {
            SourceFinding::SourceWarnings { .. } => {}
            SourceFinding::RefreshFailed { error, reason, .. } => {
                return Err(EngineError {
                    reason: *reason,
                    ..EngineError::new(*error)
                });
            }
            SourceFinding::Unknown => return Err(EngineError::new(ErrorCode::SourceUnavailable)),
        }
    }
    Ok(None)
}

/// The one redirect accepted: HTTPS to exactly one named service, nothing else.
fn accepted(location: Option<&str>) -> Result<(LookupService, url::Url), EngineError> {
    let url = location
        .and_then(|location| url::Url::parse(location).ok())
        .ok_or_else(malformed)?;
    let service = LookupService::ALL
        .into_iter()
        .find(|service| url.host_str() == Some(service.host()))
        .ok_or_else(malformed)?;
    let plain = url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none();
    plain.then_some((service, url)).ok_or_else(malformed)
}

/// One CSL-JSON object for this DOI, cut to bibliographic CSL variables and
/// read by the same rules as records sent with a request.
fn checked(answer: &[u8], doi: &str) -> Result<Value, EngineError> {
    let Ok(Value::Object(fields)) = serde_json::from_slice::<Value>(answer) else {
        return Err(malformed());
    };
    let mut record: Map<String, Value> = ibis_sources::bibliographic_fields(fields);
    if let Some(Value::String(kind)) = record.get_mut("type")
        && let Some(csl) = crossref_type(kind)
    {
        *kind = csl.to_owned();
    }
    match record.get("DOI").and_then(Value::as_str) {
        // A service may answer for another DOI only by mistake; never cite that.
        Some(given) if identifier::parse_query(given) != Some(Identifier::Doi(doi.to_owned())) => {
            return Err(malformed());
        }
        Some(_) => {}
        None => {
            record.insert("DOI".into(), Value::String(doi.to_owned()));
        }
    }
    record.insert("id".into(), Value::String("lookup".into()));
    let record = Value::Object(record);
    let read = ibis_sources::request_records(&LibraryId::digest(b"looked up online"), std::slice::from_ref(&record))
        .map_err(|_| malformed())?;
    if read.records.len() != 1 {
        return Err(malformed());
    }
    Ok(record)
}

/// Crossref's CSL answers name its own work types; these are the CSL types they
/// mean. Any other type must already be CSL's, or the record is refused.
fn crossref_type(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "journal-article" => "article-journal",
        "book-chapter" | "book-section" | "book-part" => "chapter",
        "proceedings-article" => "paper-conference",
        "posted-content" => "article",
        "dissertation" => "thesis",
        "report-component" => "report",
        "monograph" | "edited-book" | "reference-book" => "book",
        "reference-entry" => "entry",
        "database" => "dataset",
        "peer-review" => "review",
        _ => return None,
    })
}

fn refusal(code: ErrorCode, reason: ErrorReason) -> EngineError {
    EngineError::because(code, reason)
}
fn malformed() -> EngineError {
    refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupMalformed)
}
fn status(code: u16) -> EngineError {
    match code {
        404 | 410 => refusal(ErrorCode::NotFound, ErrorReason::DoiNotFound),
        429 => refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupRateLimited),
        // A record or a redirect where neither belongs.
        200..=399 => malformed(),
        _ => refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupUnavailable),
    }
}
fn failed(failure: Failure) -> EngineError {
    match failure {
        Failure::Timeout => refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupTimeout),
        Failure::Unreachable => refusal(ErrorCode::SourceUnavailable, ErrorReason::LookupUnreachable),
    }
}
fn settings_error(problem: &SettingsError) -> EngineError {
    EngineError::new(crate::settings_backup::settings_code(problem))
}

/// One answer, with only what the lookup reads from it.
pub(crate) struct Reply {
    pub status: u16,
    pub location: Option<String>,
    pub content_type: Option<String>,
    pub body: Box<dyn Read + Send>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    Unreachable,
    Timeout,
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        let timeout = error.kind() == std::io::ErrorKind::TimedOut
            || error
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<ureq::Error>())
                .is_some_and(|inner| matches!(inner, ureq::Error::Timeout(_)));
        if timeout { Self::Timeout } else { Self::Unreachable }
    }
}

/// A GET of `url` that never follows a redirect and gives up after `within`.
pub(crate) trait Transport: Send + Sync {
    fn get(&self, url: &str, within: Duration) -> Result<Reply, Failure>;
}

/// The real client: rustls with the system's trust store, as updates use.
pub(crate) struct Https {
    agent: ureq::Agent,
}

impl Https {
    /// `https_only` is false only for tests against a local plain-HTTP server.
    pub(crate) fn new(https_only: bool) -> Self {
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .https_only(https_only)
            .proxy(None)
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .accept(CSL_JSON)
            .timeout_connect(Some(CONNECT))
            .max_idle_connections(0)
            .tls_config(tls)
            .build()
            .new_agent();
        Self { agent }
    }
}

impl Transport for Https {
    fn get(&self, url: &str, within: Duration) -> Result<Reply, Failure> {
        let response = self
            .agent
            .get(url)
            .config()
            .timeout_global(Some(within))
            .build()
            .call()
            .map_err(|error| match error {
                ureq::Error::Timeout(_) => Failure::Timeout,
                ureq::Error::Io(error) => Failure::from(error),
                _ => Failure::Unreachable,
            })?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let (location, content_type) = (header("location"), header("content-type"));
        Ok(Reply {
            status: response.status().as_u16(),
            location,
            content_type,
            body: Box::new(response.into_body().into_reader()),
        })
    }
}

#[cfg(test)]
impl Lookup {
    pub(crate) fn with_transport(transport: impl Transport + 'static) -> Self {
        Self {
            transport: Box::new(transport),
        }
    }
}

#[cfg(test)]
#[path = "lookup/tests.rs"]
mod tests;
