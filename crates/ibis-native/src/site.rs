//! The website in front in a browser, kept only as its site name, so a
//! format can be remembered for Overleaf apart from Google Docs (decision
//! 0031).
//!
//! It is read once per picker opening, before the panel shows, and only when
//! the app in front is a browser the destination table lists. It uses only
//! what Ibis can already read: on macOS the Accessibility grant, checked
//! without asking; on Windows UI Automation, which needs none. It never asks
//! for a permission, sends Apple Events or turns a browser setting on, and
//! it uses no network. The address is reduced to its site name inside
//! [`read_with`]; no path, query or full address is kept, logged or returned.
//! When anything is missing or slow, there is no site and the browser's own
//! format applies, as for any app.

use std::time::{Duration, Instant};

use ibis_core::destination::{AppId, is_browser, site};

/// The longest the read may hold the picker's opening, as one paste query
/// (decision 0029).
pub const LIMIT: Duration = Duration::from_millis(50);
/// The second look at the window, after the paste capture.
const CONFIRM: Duration = Duration::from_millis(15);

/// What was in front when the picker opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Front {
    /// Ibis itself: the earlier opening's site still holds, as its app does.
    Ibis,
    /// Another app and its process, with the website in front when it is a
    /// browser whose front tab could be read.
    App { app: AppId, pid: i32, site: Option<AppId> },
}

/// The platform calls behind [`read`], so tests can stand in for them.
pub trait Source {
    /// A window's identity, equal only to the same window.
    type Window: PartialEq;
    /// The app in front: its process and its ID. Ibis's own process when
    /// one of its windows has the keyboard, as the paste capture counts it.
    fn front(&self) -> Option<(i32, AppId)>;
    /// Whether Ibis may read other apps' controls, checked without asking.
    fn allowed(&self) -> bool;
    /// `pid`'s focused window, by `deadline`.
    fn window(&self, pid: i32, deadline: Instant) -> Option<Self::Window>;
    /// The address of the page `window` shows, by `deadline`. Only
    /// [`read_with`] sees it.
    fn address(&self, window: &Self::Window, deadline: Instant) -> Option<String>;
}

/// The app in front and its site, read through `source` around `capture`,
/// the paste capture, which always runs once and whose answer comes back
/// beside it; `own` is Ibis's process. The site counts only when the same
/// window is focused before and after the capture, so it is the window the
/// capture took. No front when the app in front cannot be told.
pub fn read_with<S: Source, T>(source: &S, own: i32, capture: impl FnOnce() -> T) -> (Option<Front>, T) {
    let Some((pid, app)) = source.front() else {
        return (None, capture());
    };
    if pid == own {
        return (Some(Front::Ibis), capture());
    }
    let deadline = Instant::now() + LIMIT;
    let read = (is_browser(&app) && source.allowed())
        .then(|| source.window(pid, deadline))
        .flatten()
        .and_then(|window| {
            let site = source.address(&window, deadline).and_then(|address| site(&address))?;
            Some((window, site))
        });
    let captured = capture();
    let site = read.and_then(|(window, site)| {
        let same = source.front().is_some_and(|(now, _)| now == pid)
            && source.window(pid, Instant::now() + CONFIRM).as_ref() == Some(&window);
        same.then_some(site)
    });
    (Some(Front::App { app, pid, site }), captured)
}

/// [`read_with`] on this system.
#[cfg(target_os = "macos")]
pub fn read<T>(own: i32, capture: impl FnOnce() -> T) -> (Option<Front>, T) {
    read_with(&macos::Native, own, capture)
}

/// [`read_with`] on this system.
#[cfg(target_os = "windows")]
pub fn read<T>(own: i32, capture: impl FnOnce() -> T) -> (Option<Front>, T) {
    read_with(&windows::Native, own, capture)
}

/// Linux has no permission-free source, so there a browser is one app.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn read<T>(_own: i32, capture: impl FnOnce() -> T) -> (Option<Front>, T) {
    (None, capture())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::System;
    use std::cell::{Cell, RefCell};

    struct Fake {
        front: Option<(i32, AppId)>,
        allowed: bool,
        address: Option<&'static str>,
        /// The focused window at each look, in turn; the last one repeats.
        windows: RefCell<Vec<u32>>,
        asked: Cell<u32>,
        deadline: RefCell<Option<Instant>>,
    }

    impl Fake {
        fn new(system: System, id: &str, address: Option<&'static str>) -> Self {
            Self {
                front: Some((42, AppId { system, id: id.into() })),
                allowed: true,
                address,
                windows: RefCell::new(vec![1]),
                asked: Cell::new(0),
                deadline: RefCell::new(None),
            }
        }
    }

    impl Source for Fake {
        type Window = u32;
        fn front(&self) -> Option<(i32, AppId)> {
            self.front.clone()
        }
        fn allowed(&self) -> bool {
            self.allowed
        }
        fn window(&self, _pid: i32, _deadline: Instant) -> Option<u32> {
            let mut windows = self.windows.borrow_mut();
            if windows.len() > 1 {
                Some(windows.remove(0))
            } else {
                windows.first().copied()
            }
        }
        fn address(&self, _window: &u32, deadline: Instant) -> Option<String> {
            self.asked.set(self.asked.get() + 1);
            self.deadline.replace(Some(deadline));
            self.address.map(str::to_owned)
        }
    }

    /// Reads around a stand-in capture, which must run exactly once.
    fn read(fake: &Fake, own: i32) -> Option<Front> {
        let (front, captured) = read_with(fake, own, || "captured");
        assert_eq!(captured, "captured");
        front
    }

    fn site_of(front: Option<Front>) -> Option<AppId> {
        match front {
            Some(Front::App { site, .. }) => site,
            _ => None,
        }
    }

    #[test]
    fn a_browser_gives_only_the_site_name_of_its_front_tab() {
        let fake = Fake::new(
            System::Macos,
            "com.google.Chrome",
            Some("https://www.overleaf.com/project/0000?token=invented#x"),
        );
        let started = Instant::now();
        let site = site_of(read(&fake, 7));
        assert_eq!(
            site,
            Some(AppId {
                system: System::Web,
                id: "overleaf.com".into()
            })
        );
        let deadline = fake.deadline.borrow().unwrap_or(started);
        assert!(deadline > started && deadline <= Instant::now() + LIMIT);
        let windows = Fake::new(
            System::Windows,
            "msedge.exe",
            Some("docs.google.com/document/d/invented"),
        );
        assert_eq!(
            site_of(read(&windows, 7)).map(|site| site.id),
            Some("docs.google.com".into())
        );
    }

    #[test]
    fn a_window_switched_during_the_capture_gives_no_site() {
        let fake = Fake::new(System::Macos, "com.google.Chrome", Some("https://overleaf.com/"));
        fake.windows.replace(vec![1, 2]);
        assert!(matches!(read(&fake, 7), Some(Front::App { site: None, .. })));
    }

    #[test]
    fn other_apps_are_never_read() {
        // Electron apps hold web pages too; only listed browsers are asked.
        for id in [
            "md.obsidian",
            "com.microsoft.VSCode",
            "notion.id",
            "org.example.invented",
        ] {
            let fake = Fake::new(System::Macos, id, Some("https://overleaf.com/"));
            assert_eq!(
                read(&fake, 7),
                Some(Front::App {
                    app: AppId {
                        system: System::Macos,
                        id: id.into()
                    },
                    pid: 42,
                    site: None
                })
            );
            assert_eq!(fake.asked.get(), 0, "{id}");
        }
    }

    #[test]
    fn without_the_grant_or_a_readable_page_the_browser_is_one_app() {
        let mut fake = Fake::new(System::Macos, "com.apple.Safari", Some("https://overleaf.com/"));
        fake.allowed = false;
        assert_eq!(site_of(read(&fake, 7)), None);
        assert_eq!(fake.asked.get(), 0);
        for address in [
            None,
            Some("file:///Users/invented/notes.html"),
            Some("about:blank"),
            Some(""),
        ] {
            let fake = Fake::new(System::Macos, "com.apple.Safari", address);
            assert!(matches!(read(&fake, 7), Some(Front::App { site: None, .. })));
        }
    }

    #[test]
    fn ibis_in_front_keeps_the_earlier_site_and_nothing_in_front_says_so() {
        let fake = Fake::new(System::Macos, "com.google.Chrome", Some("https://overleaf.com/"));
        assert_eq!(read(&fake, 42), Some(Front::Ibis));
        assert_eq!(fake.asked.get(), 0);
        let nothing = Fake {
            front: None,
            ..Fake::new(System::Macos, "com.google.Chrome", None)
        };
        assert_eq!(read(&nothing, 7), None);
    }
}
