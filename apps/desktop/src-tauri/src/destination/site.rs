//! The website in front in the browser the picker was opened from, read
//! beside the paste capture and bound to the same app and process (decision
//! 0031). It chooses only the format; paste choices, projects and proofs stay
//! with the browser. Only the site name is held, in memory, for the latest
//! opening.

use std::sync::Mutex;

use ibis_core::destination::AppId;
use ibis_native::site::Front;
use tauri::{AppHandle, Manager, Runtime};

use crate::paste::Captured;

/// The site at the latest opening from another app, with the browser and
/// process it was read in.
struct Read {
    browser: AppId,
    pid: i32,
    site: AppId,
}

#[derive(Default)]
pub(crate) struct FrontSite(Mutex<Option<Read>>);

impl FrontSite {
    /// Ibis in front keeps the earlier site, as the paste capture keeps the
    /// earlier app; anything else replaces it, with no site when none was read.
    pub(super) fn record(&self, front: Option<Front>) {
        let Ok(mut held) = self.0.lock() else { return };
        match front {
            Some(Front::Ibis) => {}
            Some(Front::App { app, pid, site }) => {
                *held = site.map(|site| Read {
                    browser: app,
                    pid,
                    site,
                });
            }
            None => *held = None,
        }
    }

    /// The site read in `app`'s process `pid`, when that is the captured target.
    fn of(&self, app: &AppId, pid: Option<i32>) -> Option<AppId> {
        let held = self.0.lock().ok()?;
        held.as_ref()
            .filter(|read| {
                Some(read.pid) == pid
                    && read.browser.system == app.system
                    && read.browser.normalized() == app.normalized()
            })
            .map(|read| read.site.clone())
    }
}

/// Read the site in front around `capture`, the paste capture, before the
/// picker shows; `capture` runs once and its answer is returned.
pub(crate) fn capture<T>(app: &AppHandle, capture: impl FnOnce() -> T) -> T {
    let own = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    let (front, captured) = ibis_native::site::read(own, capture);
    if let Some(sites) = app.try_state::<FrontSite>() {
        sites.record(front);
    }
    captured
}

/// The website in front in a captured target, if one was read for it.
pub(crate) fn of_capture<R: Runtime>(app: &AppHandle<R>, captured: &Captured) -> Option<AppId> {
    let id = captured.target.app.as_ref()?;
    app.try_state::<FrontSite>()?.of(id, captured.pid)
}

/// The website in front in `id`, when `id` is the app the picker was last
/// opened from.
pub(crate) fn current<R: Runtime>(app: &AppHandle<R>, id: &AppId) -> Option<AppId> {
    app.try_state::<crate::paste::LastTarget>()?
        .with_captured(|captured| {
            let captured = captured?;
            let target = captured.target.app.as_ref()?;
            (target.system == id.system && target.normalized() == id.normalized())
                .then(|| of_capture(app, captured))
                .flatten()
        })
        .flatten()
}

/// A choice made for the site the picker showed still names the site in
/// front: `shown` is the site ID the picker had, `None` for none.
pub(crate) fn unchanged(shown: Option<&str>, current: Option<&AppId>) -> bool {
    shown == current.map(|site| site.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::System;

    fn chrome() -> AppId {
        AppId {
            system: System::Macos,
            id: "com.google.Chrome".into(),
        }
    }

    fn overleaf() -> Option<AppId> {
        ibis_core::destination::site("https://www.overleaf.com/project/0000")
    }

    fn front(site: Option<AppId>) -> Option<Front> {
        Some(Front::App {
            app: chrome(),
            pid: 42,
            site,
        })
    }

    #[test]
    fn a_site_belongs_to_the_browser_process_it_was_read_in_and_ibis_keeps_it() {
        let sites = FrontSite::default();
        sites.record(front(overleaf()));
        let normalized = AppId {
            system: System::Macos,
            id: "com.google.chrome".into(),
        };
        assert_eq!(sites.of(&normalized, Some(42)), overleaf());
        // Another process of the same browser, or no process, is not it.
        assert_eq!(sites.of(&chrome(), Some(43)), None);
        assert_eq!(sites.of(&chrome(), None), None);
        let safari = AppId {
            system: System::Macos,
            id: "com.apple.Safari".into(),
        };
        assert_eq!(sites.of(&safari, Some(42)), None);
        // The picker opened over Ibis: the earlier capture holds.
        sites.record(Some(Front::Ibis));
        assert_eq!(sites.of(&chrome(), Some(42)), overleaf());
        // A browser whose site could not be read is one app again.
        sites.record(front(None));
        assert_eq!(sites.of(&chrome(), Some(42)), None);
        sites.record(front(overleaf()));
        sites.record(None);
        assert_eq!(sites.of(&chrome(), Some(42)), None);
    }

    #[test]
    fn a_choice_made_for_one_site_is_refused_once_another_is_in_front() {
        let docs = ibis_core::destination::site("https://docs.google.com/");
        assert!(unchanged(Some("overleaf.com"), overleaf().as_ref()));
        assert!(!unchanged(Some("overleaf.com"), docs.as_ref()));
        assert!(!unchanged(None, docs.as_ref()));
        assert!(!unchanged(Some("overleaf.com"), None));
        assert!(unchanged(None, None));
    }
}
