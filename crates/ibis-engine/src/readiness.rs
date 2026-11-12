//! The same deliberately small, path-free readiness view for MCP and setup UI.
use crate::{Engine, LibraryInfo};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub version: &'static str,
    pub libraries_chosen: bool,
    pub libraries: Vec<Library>,
    pub health: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Library {
    pub display_name: String,
    pub item_count: usize,
    pub health: String,
}

impl Status {
    pub fn empty(chosen: bool, health: &str) -> Self {
        Self {
            version: ibis_core::VERSION,
            libraries_chosen: chosen,
            libraries: vec![],
            health: health.into(),
        }
    }
}

pub fn library_health(library: &LibraryInfo) -> &'static str {
    if library.refresh_error.is_some() {
        "could not refresh; showing the last successful read"
    } else if library.warnings_total > 0 {
        "some references could not be read"
    } else if library.objects == 0 {
        "empty"
    } else {
        "ready"
    }
}

/// Display names are fixed words, never file names, paths or source error text.
/// The engine has no person-assigned library names; distinguish files by number.
pub fn from_engine(engine: &Engine, chosen: Option<bool>, unavailable: bool) -> Status {
    let Ok(libraries) = engine.libraries() else {
        return Status::empty(chosen.unwrap_or(false), "libraries are not available");
    };
    let mut status = Status::empty(
        chosen.unwrap_or(!libraries.libraries.is_empty()),
        if unavailable {
            "libraries are not available"
        } else {
            "ready"
        },
    );
    for (index, library) in libraries.libraries.iter().enumerate() {
        let display_name = match &library.zotero {
            Some(crate::request::ZoteroLibrary::Personal) => "Zotero personal library".into(),
            Some(crate::request::ZoteroLibrary::Group(id)) => format!("Zotero group {id}"),
            None => format!("Reference library {}", index + 1),
        };
        status.libraries.push(Library {
            display_name,
            item_count: library.objects,
            health: library_health(library).into(),
        });
    }
    if !status.libraries_chosen {
        status.health = "no libraries chosen".into();
    }
    status
}
