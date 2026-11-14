//! Values whose output syntax carries identity, rather than prose.
use serde_json::{Map, Value};
use std::net::IpAddr;

/// A decoded URL that can be opened outside this device and its network.
#[derive(Debug)]
pub struct PublicUrl(String);

impl PublicUrl {
    pub fn new(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() || value.contains('\\') || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return None;
        }
        let parsed = if let Some((scheme, rest)) = value.split_once(':') {
            match scheme.to_ascii_lowercase().as_str() {
                "http" | "https" | "ftp" if rest.starts_with("//") => url::Url::parse(value).ok()?,
                "doi" => return DoiLink::new(rest).map(|_| Self(value.into())),
                "mailto" => {
                    let (_, host) = rest.split(['?', '#']).next()?.rsplit_once('@')?;
                    let parsed = url::Url::parse(&format!("https://{host}")).ok()?;
                    return public_host(&parsed).then(|| Self(value.into()));
                }
                "hdl" | "urn" if !rest.is_empty() && !rest.starts_with('/') && !rest.contains("://") => {
                    return Some(Self(value.into()));
                }
                _ => return None,
            }
        } else {
            let parsed = url::Url::parse(&format!("https://{value}")).ok()?;
            let top = parsed.host_str()?.rsplit('.').next()?;
            if super::DOCUMENT_SUFFIXES.contains(&top.to_ascii_lowercase().as_str()) {
                return None;
            }
            parsed
        };
        public_host(&parsed).then(|| Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn public_host(url: &url::Url) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match url.host() {
        Some(url::Host::Ipv4(address)) => global_address(address.into()),
        Some(url::Host::Ipv6(address)) => global_address(address.into()),
        Some(url::Host::Domain(host)) => {
            let host = host.trim_end_matches('.');
            host.contains('.')
                && !["localhost", "local", "internal", "lan", "home", "home.arpa"]
                    .iter()
                    .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
                && host
                    .split('.')
                    .all(|label| !label.is_empty() && label.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
        }
        None => false,
    }
}

// RFC 6890 special-purpose ranges, including multicast and reserved space.
// IPv6 output is restricted to native global unicast: transition addresses
// must not smuggle a private IPv4 address through a globally shaped prefix.
fn global_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let [a, b, c, d] = address.octets();
            !matches!((a, b, c, d),
                (0 | 10 | 127 | 224..=255, _, _, _)
                | (100, 64..=127, _, _) | (169, 254, _, _) | (172, 16..=31, _, _)
                | (192, 0, 0, 0..=8 | 11..=255) | (192, 0, 2, _) | (192, 88, 99, _)
                | (192, 168, _, _) | (198, 18..=19, _, _) | (198, 51, 100, _) | (203, 0, 113, _))
        }
        IpAddr::V6(address) => {
            let [a, b, ..] = address.segments();
            matches!(a, 0x2000..=0x3fff)
                && !(a == 0x2001 && (b < 0x200 || b == 0xdb8))
                && a != 0x2002
                && !(a == 0x3fff && b < 0x1000)
        }
    }
}

/// One DOI identity and its resolver URL. No serializer may reinterpret it.
#[derive(Debug, Clone)]
pub struct DoiLink {
    identifier: String,
    link: String,
}

impl DoiLink {
    pub fn new(value: &str) -> Option<Self> {
        let mut doi = value.trim();
        while let Some(bare) = [
            "https://doi.org/",
            "http://doi.org/",
            "https://dx.doi.org/",
            "http://dx.doi.org/",
            "doi.org/",
            "dx.doi.org/",
            "doi:",
        ]
        .iter()
        .find_map(|prefix| {
            doi.get(..prefix.len())
                .filter(|head| head.eq_ignore_ascii_case(prefix))
                .map(|_| doi[prefix.len()..].trim())
        }) {
            doi = bare;
        }
        let (prefix, suffix) = doi.split_once('/')?;
        let registrant = prefix.strip_prefix("10.")?;
        if doi.len() > 64 * 1024
            || registrant.len() < 4
            || !registrant.bytes().all(|c| c.is_ascii_digit() || c == b'.')
            || suffix.is_empty()
            || doi.chars().any(|c| c.is_whitespace() || c.is_control() || c == '\\')
            || doi.split('/').any(|segment| matches!(segment, "." | ".."))
            || super::points_to_device(doi)
        {
            return None;
        }
        let mut link = String::from("https://doi.org/");
        for byte in doi.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
                link.push(char::from(byte));
            } else {
                use std::fmt::Write;
                let _ = write!(link, "%{byte:02X}");
            }
        }
        Some(Self {
            identifier: doi.into(),
            link,
        })
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    pub fn as_str(&self) -> &str {
        &self.link
    }
}

/// A BibLaTeX verbatim value. Balanced braces are data; backslashes and
/// unbalanced delimiters are refused, never converted into prose commands.
#[derive(Debug)]
pub struct Verbatim(String);

impl Verbatim {
    pub(super) fn new(value: &str) -> Option<Self> {
        let mut depth = 0usize;
        for c in value.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth = depth.checked_sub(1)?,
                '\\' => return None,
                c if c.is_control() => return None,
                _ => {}
            }
        }
        (depth == 0).then(|| Self(value.into()))
    }

    pub(crate) fn key(value: &str) -> Option<Self> {
        (!value.is_empty() && value.chars().all(|c| c.is_alphanumeric() || "-_.:+/".contains(c)))
            .then(|| Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// All CSL name parts survive normalization, before any serializer sees them.
#[derive(Debug, serde::Serialize)]
pub struct ExportName {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(rename = "dropping-particle", skip_serializing_if = "Option::is_none")]
    dropping: Option<String>,
    #[serde(rename = "non-dropping-particle", skip_serializing_if = "Option::is_none")]
    non_dropping: Option<String>,
    #[serde(flatten)]
    other_parts: Map<String, Value>,
}

impl ExportName {
    pub(super) fn new(mut parts: Map<String, Value>) -> Self {
        // Preserve scalar type errors for the native CSL reader to refuse;
        // normalization cannot turn a malformed author into an anonymous one.
        let mut text = |key| {
            let text = parts.get(key)?.as_str()?.to_owned();
            parts.remove(key);
            Some(text)
        };
        Self {
            family: text("family"),
            given: text("given"),
            literal: text("literal"),
            suffix: text("suffix"),
            dropping: text("dropping-particle"),
            non_dropping: text("non-dropping-particle"),
            other_parts: parts,
        }
    }

    /// BibLaTeX has one prefix. With both particles, keep the non-dropping
    /// particle in the protected family and the dropping particle in prefix.
    /// Neither becomes a given name that a reader could turn into initials.
    pub fn bibtex_parts(&self) -> (String, &str, Option<(&str, bool)>) {
        let family = self.family.as_deref().unwrap_or_default();
        let given = self.given.as_deref().unwrap_or_default();
        match (
            self.non_dropping.as_deref().filter(|s| !s.is_empty()),
            self.dropping.as_deref().filter(|s| !s.is_empty()),
        ) {
            (Some(non_dropping), Some(dropping)) => (
                [non_dropping, family]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
                given,
                Some((dropping, false)),
            ),
            (Some(prefix), None) => (family.into(), given, Some((prefix, true))),
            (None, dropping) => (family.into(), given, dropping.map(|prefix| (prefix, false))),
        }
    }
}
