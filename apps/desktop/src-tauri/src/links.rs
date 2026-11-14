//! macOS URL ingress reuses the Search service's one pending request.

#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::{AppHandle, Manager, Runtime};

use crate::tray::SearchRequest;

pub(crate) fn receive<R: Runtime>(app: &AppHandle<R>, link: &str) -> bool {
    let Some(query) = ibis_engine::links::parse(link) else {
        return false;
    };
    let request = app.state::<SearchRequest>();
    let Ok(mut waiting) = request.0.lock() else {
        return false;
    };
    // Acceptance revokes delivery before the interface can consume the text.
    // The destination owner keeps its existing target and policy; no capture.
    if crate::paste::change_destination(app, true, |_| Ok(())).is_err() {
        return false;
    }
    *waiting = Some(query);
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn present(app: &AppHandle) {
    use crate::diagnostics::{NativeIssue, report};

    // Supersede a shortcut show still waiting on native work. The shortcut's
    // show_picker captures a target, so use the panel owner directly here.
    let owner = app.state::<crate::picker::PickerGeneration>().advance();
    crate::updater::picker_activity(app);
    crate::notice::retire(app);
    crate::settings_window::picker_opened();
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if !crate::sizing::prepare(app, &window, false, false)
        || app.state::<crate::picker::PickerGeneration>().current() != owner
    {
        return;
    }
    report(
        app.emit_to("main", "picker:preparing", Option::<String>::None),
        NativeIssue::PickerEvent,
    );
    crate::native_feel::before_show(app);
    report(window.unminimize(), NativeIssue::Unminimize);
    crate::panel::show(app);
    report(window.emit("picker:shown", owner), NativeIssue::PickerEvent);
    report(
        app.emit_to("main", "picker:search-requested", ()),
        NativeIssue::PickerEvent,
    );
    crate::library::refresh_zotero(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::take_search_request;

    #[test]
    fn links_wait_once_keep_the_newest_query_and_create_no_delivery_target() -> Result<(), Box<dyn std::error::Error>> {
        let app = tauri::test::mock_builder()
            .manage(SearchRequest::default())
            .manage(crate::paste::LastTarget::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
        assert!(receive(app.handle(), "ibis://search?q=%E9%9B%AA+%26+caf%C3%A9%3F"));
        assert_eq!(take_search_request(app.state()), Some("雪 & café?".into()));
        assert_eq!(take_search_request(app.state()), None);
        assert!(receive(app.handle(), "ibis://search?q=older"));
        assert!(receive(app.handle(), "ibis://search?q=newest"));
        assert_eq!(take_search_request(app.state()), Some("newest".into()));
        assert_eq!(take_search_request(app.state()), None);
        assert!(crate::paste::change_destination(app.handle(), false, |target| Ok(
            target.is_none()
        ))?);
        Ok(())
    }

    #[test]
    fn links_refuse_without_touching_a_waiting_search() -> Result<(), Box<dyn std::error::Error>> {
        let app = tauri::test::mock_builder()
            .manage(SearchRequest::default())
            .manage(crate::paste::LastTarget::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
        assert!(receive(app.handle(), "ibis://search?q=waiting"));
        for link in [
            "ibis://paste?q=bad",
            "ibis://search?q=%",
            "ibis://search?q=%FF",
            "ibis://search?q=one&q=two",
            "ibis://user@search?q=bad",
            "ibis://search:123?q=bad",
            "ibis://search?q=bad&paste=true",
            "ibis://search?q=bad&path=/invented/library.bib",
            "ibis://search?q=bad&library=/invented/library.bib",
            "ibis://search?q=bad&output=latex",
        ] {
            assert!(!receive(app.handle(), link));
        }
        assert_eq!(take_search_request(app.state()), Some("waiting".into()));
        Ok(())
    }
}
