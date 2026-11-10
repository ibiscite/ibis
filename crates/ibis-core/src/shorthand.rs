//! What a person types around a reference in one search, read by the engine so
//! every front end means the same thing: a leading signal (`see`, `cf.`) that
//! becomes the reference's prefix, and a trailing locator (`417`, `ch 2`).
//!
//! Only these forms are read, so a plain search is never changed by guesswork:
//! - a signal from a short fixed list, followed by at least one more word;
//! - a locator as [`crate::locator::parse_trailing`] reads it.
//!
//! The caller shows what was read so the person can undo it.

use crate::locator::{TrailingLocator, parse_trailing};

/// Signals that open a citation, longest first so `see also` wins over `see`.
const SIGNALS: [&str; 11] = [
    "see, e.g.,",
    "see also",
    "but see",
    "but cf.",
    "e.g.,",
    "e.g.",
    "compare",
    "contra",
    "see",
    "cf.",
    "cf",
];

/// A search with its typed signal and locator split off.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shorthand {
    /// What remains to search for.
    pub search: String,
    /// The signal exactly as typed, to become the reference's prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<TrailingLocator>,
}

/// Read `query`, or `None` when it holds no signal and no locator.
pub fn read(query: &str) -> Option<Shorthand> {
    let trimmed = query.trim_start();
    let lower = trimmed.to_lowercase();
    let signal = SIGNALS.iter().find_map(|signal| {
        let rest = trimmed.get(signal.len()..)?;
        // A signal is whole words, then a space and more to search for.
        (lower.starts_with(signal) && rest.starts_with(char::is_whitespace) && !rest.trim().is_empty())
            .then(|| (trimmed[..signal.len()].to_string(), rest.trim_start()))
    });
    let (prefix, rest) = signal.map_or((None, trimmed), |(prefix, rest)| (Some(prefix), rest));
    let locator = parse_trailing(rest);
    if prefix.is_none() && locator.is_none() {
        return None;
    }
    let search = locator
        .as_ref()
        .map_or_else(|| rest.trim_end().to_string(), |read| read.search.clone());
    Some(Shorthand {
        search,
        prefix,
        locator,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::LocatorTerm;

    fn read_all(query: &str) -> Option<(String, Option<String>, Option<(LocatorTerm, String)>)> {
        read(query).map(|s| (s.search, s.prefix, s.locator.map(|l| (l.locator.term, l.locator.value))))
    }

    #[test]
    fn a_signal_and_a_locator_are_read_around_the_search() {
        let page = |value: &str| Some((LocatorTerm::Page, value.to_string()));
        assert_eq!(
            read_all("see vale 417"),
            Some(("vale".into(), Some("see".into()), page("417")))
        );
        assert_eq!(
            read_all("vale ch 2"),
            Some(("vale".into(), None, Some((LocatorTerm::Chapter, "2".into()))))
        );
        assert_eq!(
            read_all("See also okafor"),
            Some(("okafor".into(), Some("See also".into()), None))
        );
        assert_eq!(
            read_all("cf. lin 2019 pp. 3-5"),
            Some(("lin 2019".into(), Some("cf.".into()), page("3-5")))
        );
        assert_eq!(read_all("vale 12–14"), Some(("vale".into(), None, page("12–14"))));
    }

    #[test]
    fn plain_searches_are_never_changed() {
        // A signal alone, a signal that is part of a word, a year and a plain query.
        for query in ["see", "see ", "seeing red", "cfg files", "vale 2021", "vale", "compare"] {
            assert_eq!(read(query), None, "{query:?}");
        }
    }
}
