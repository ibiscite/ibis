//! Zotero library identity, local API transport and group discovery.

use std::fmt;

pub mod api;
pub mod libraries;

#[cfg(test)]
mod test_support;

/// Which Zotero library items belong to. Identity is the library plus the
/// item key, never a citation key (ADR 0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Library {
    /// The personal library ("My Library").
    User,
    /// A group library, by its numeric group ID.
    Group(u64),
}

impl fmt::Display for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Library::User => f.write_str("zotero:user"),
            Library::Group(id) => write!(f, "zotero:group:{id}"),
        }
    }
}
