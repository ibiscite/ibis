//! A website as Ibis keeps it: its site name, such as `overleaf.com`, and
//! nothing else (decision 0031). The platform layer reads the front tab's
//! address and passes it straight to [`site`], so a path, query, fragment,
//! sign-in name or port never leaves the read that found it.

use super::{APPS, App, AppId, System};

/// The longest host name DNS allows.
const MAX_HOST: usize = 253;

/// The site an address belongs to, from a browser's page address or the text
/// in its address field (which may leave out `https://`). Only web pages
/// have one: `file:`, `about:`, browser pages, IP addresses and single-label
/// hosts such as `localhost` have none, and nor does anything with a sign-in
/// name, which could hide the real host. A site Ibis lists is named as its
/// table entry, so `www.overleaf.com` and `eu.overleaf.com` are one site;
/// any other keeps its whole host without `www.`, so `docs.google.com` stays
/// apart from `mail.google.com`.
pub fn site(address: &str) -> Option<AppId> {
    let address = address.trim();
    if address.is_empty() || address.len() > 4096 || address.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    // Parsed as a browser parses it, so the host is the one the page is on.
    let parsed = if address.contains("://") {
        url::Url::parse(address)
    } else {
        url::Url::parse(&format!("https://{address}"))
    }
    .ok()?;
    if !matches!(parsed.scheme(), "https" | "http") || !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    let url::Host::Domain(host) = parsed.host()? else {
        return None;
    };
    Some(AppId {
        system: System::Web,
        id: name(host)?,
    })
}

/// The site name for a host, as [`site`] keeps it; `None` for anything that
/// is not a domain name.
pub(super) fn name(host: &str) -> Option<String> {
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let labels: Vec<&str> = host.split('.').collect();
    let valid = |label: &&str| {
        (1..=63).contains(&label.len())
            && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    };
    let top = labels.last()?;
    if host.len() > MAX_HOST || labels.len() < 2 || !labels.iter().all(valid) || top.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some(row(host).map_or_else(|| host.to_owned(), |entry| entry.id.to_owned()))
}

/// The listed site that holds `host`: the host itself or the nearest domain
/// above it, never a bare top-level domain.
pub(super) fn row(host: &str) -> Option<&'static App> {
    let mut rest = host;
    loop {
        if let Some(entry) = APPS
            .iter()
            .find(|entry| entry.system == System::Web && entry.id == rest)
        {
            return Some(entry);
        }
        rest = rest.split_once('.')?.1;
        if !rest.contains('.') {
            return None;
        }
    }
}
