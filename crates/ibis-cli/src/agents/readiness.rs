use crate::app_libraries;
use ibis_engine::{
    Cancellation, Engine, Method,
    readiness::{Library, Status, from_engine},
};
use serde_json::json;

/// Inspect app choices through the existing app-libraries loader. Source errors
/// become fixed words; settings, paths, notes and links never enter this view.
pub fn readiness() -> Status {
    crate::bounded(inspect).unwrap_or_else(|| Status::empty(false, "library readiness could not be checked in time"))
}

fn inspect() -> Status {
    let app = match app_libraries::load() {
        Ok(app) => app,
        Err(error) => {
            return Status::empty(
                false,
                if error.error.code == ibis_core::contract::ErrorCode::Usage {
                    "no libraries chosen"
                } else {
                    "Ibis settings are not available; open or update Ibis"
                },
            );
        }
    };
    let mut status = Status::empty(true, "ready");
    let engine = match Engine::new() {
        Ok(engine) => engine.without_file_watches(),
        Err(_) => return Status::empty(true, "libraries are not available"),
    };
    for (index, file) in app.files.iter().enumerate() {
        match engine.dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":file}),
            &Cancellation::default(),
        ) {
            Ok(_) => {}
            Err(_) => status.libraries.push(Library {
                display_name: format!("Reference library {}", index + 1),
                item_count: 0,
                health: "not available; choose it again in Ibis".into(),
            }),
        }
    }
    if !app.zotero.is_empty() {
        let params = json!({"action":"open-zotero","libraries":app.zotero,"profile":app.zotero_profile});
        if engine
            .dispatch(Method::Libraries, &params, &Cancellation::default())
            .is_err()
        {
            for chosen in &app.zotero {
                status.libraries.push(Library {
                    display_name: match chosen {
                        ibis_engine::request::ZoteroLibrary::Personal => "Zotero personal library".into(),
                        ibis_engine::request::ZoteroLibrary::Group(id) => format!("Zotero group {id}"),
                    },
                    item_count: 0,
                    health: "not available; choose it again in Ibis".into(),
                });
            }
        }
    }
    status
        .libraries
        .extend(from_engine(&engine, Some(true), false).libraries);
    if status
        .libraries
        .iter()
        .any(|l| l.health != "ready" && l.health != "empty")
    {
        status.health = "some libraries need attention".into();
    }
    status
}
