//! Shared source identity claims and safe errors. File loading lives in `ibis-sources`.

use std::{collections::HashMap, hash::Hash};
use thiserror::Error;

/// Source identities, claimed before validating their payloads. A rejected
/// contender still makes the identity ambiguous; no contender may win it.
pub struct IdentityClaims<K, L> {
    claims: HashMap<K, Vec<L>>,
}

impl<K: Eq + Hash, L> Default for IdentityClaims<K, L> {
    fn default() -> Self {
        Self { claims: HashMap::new() }
    }
}

impl<K: Eq + Hash, L> IdentityClaims<K, L> {
    pub fn claim(&mut self, identity: K, location: L) {
        self.claims.entry(identity).or_default().push(location);
    }

    pub fn is_unique(&self, identity: &K) -> bool {
        self.claims.get(identity).is_some_and(|locations| locations.len() == 1)
    }

    /// Every location under every repeated identity, including the first.
    pub fn duplicates(&self) -> impl Iterator<Item = (&K, &L)> {
        self.claims
            .iter()
            .filter(|(_, locations)| locations.len() > 1)
            .flat_map(|(identity, locations)| locations.iter().map(move |location| (identity, location)))
    }
}

/// Allowlisted diagnostics: never expose paths, contents or operating system errors.
/// Front ends add any context about their installed references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("the library file is unavailable; download any online-only file to this computer first")]
    Unavailable,
    #[error("the library must be a regular file")]
    NotRegular,
    #[error("the library file is larger than 64 MiB")]
    TooLarge,
    #[error("the library file is not UTF-8 text; save it as UTF-8")]
    Unreadable,
    #[error("the library file contains an unfinished entry")]
    Incomplete,
    #[error("the library exceeded the load work limit")]
    LoadLimit,
    #[error("the library file is still changing")]
    Changing,
    #[error("automatic library updates are unavailable; use Reload to try again")]
    WatchUnavailable,
}
