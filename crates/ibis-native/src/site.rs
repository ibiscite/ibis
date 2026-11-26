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

/// What was in front when the picker opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Front {
    /// Ibis itself: the earlier opening's site still holds, as its app does.
    Ibis,
    /// Another app and its process, with the website in front when it is a
    /// browser whose front tab could be read.
    App { app: AppId, pid: i32, site: Option<AppId> },
}

/// The exact window retained by paste capture, never a fresh focus query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturedWindow {
    /// The opaque token for the retained macOS AX document window.
    Macos(u32),
    /// The HWND retained for Windows restore.
    Windows(usize),
}

/// The platform calls behind [`read`], so tests can stand in for them.
pub trait Source {
    /// The app in front: its process and its ID. Ibis's own process when
    /// one of its windows has the keyboard, as the paste capture counts it.
    fn front(&self) -> Option<(i32, AppId)>;
    /// Whether Ibis may read other apps' controls, checked without asking.
    fn allowed(&self) -> bool;
    /// The address in the exact retained paste window, owned by `pid`, by
    /// `deadline`. Only [`read_with`] sees the address. Never query focus here.
    fn address(&self, window: CapturedWindow, pid: i32, deadline: Instant) -> Option<String>;
}

/// Capture once, then read the site from the window retained for paste.
/// `own` is Ibis's process; an opening from Ibis keeps the earlier site.
/// No front when the app in front cannot be told. No site when the retained
/// window is missing, belongs to another process, fails or exceeds the deadline.
pub fn read_with<S: Source, T>(
    source: &S,
    own: i32,
    capture: impl FnOnce() -> (T, Option<CapturedWindow>),
) -> (Option<Front>, T) {
    let Some((pid, app)) = source.front() else {
        return (None, capture().0);
    };
    if pid == own {
        return (Some(Front::Ibis), capture().0);
    }
    let (captured, window) = capture();
    let deadline = Instant::now() + LIMIT;
    let site = (is_browser(&app) && source.allowed())
        .then(|| window.and_then(|window| source.address(window, pid, deadline)))
        .flatten()
        .filter(|_| Instant::now() < deadline)
        .and_then(|address| site(&address));
    (Some(Front::App { app, pid, site }), captured)
}

/// [`read_with`] on this system.
#[cfg(target_os = "macos")]
pub fn read<T>(own: i32, capture: impl FnOnce() -> (T, Option<CapturedWindow>)) -> (Option<Front>, T) {
    read_with(&macos::Native, own, capture)
}

/// [`read_with`] on this system.
#[cfg(target_os = "windows")]
pub fn read<T>(own: i32, capture: impl FnOnce() -> (T, Option<CapturedWindow>)) -> (Option<Front>, T) {
    read_with(&windows::Native, own, capture)
}

/// Linux has no permission-free source, so there a browser is one app.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn read<T>(_own: i32, capture: impl FnOnce() -> (T, Option<CapturedWindow>)) -> (Option<Front>, T) {
    (None, capture().0)
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
        address: Cell<Option<&'static str>>,
        window: CapturedWindow,
        asked: Cell<u32>,
        deadline: RefCell<Option<Instant>>,
        slow: bool,
    }

    impl Fake {
        fn new(system: System, id: &str, address: Option<&'static str>) -> Self {
            Self {
                front: Some((42, AppId { system, id: id.into() })),
                allowed: true,
                address: Cell::new(address),
                window: CapturedWindow::Macos(1),
                asked: Cell::new(0),
                deadline: RefCell::new(None),
                slow: false,
            }
        }
    }

    impl Source for Fake {
        fn front(&self) -> Option<(i32, AppId)> {
            self.front.clone()
        }
        fn allowed(&self) -> bool {
            self.allowed
        }
        fn address(&self, window: CapturedWindow, pid: i32, deadline: Instant) -> Option<String> {
            self.asked.set(self.asked.get() + 1);
            self.deadline.replace(Some(deadline));
            if self.slow {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            }
            (window == self.window && Some(pid) == self.front.as_ref().map(|(pid, _)| *pid))
                .then(|| self.address.get().map(str::to_owned))
                .flatten()
        }
    }

    /// Reads from a stand-in capture, which must run exactly once.
    fn read(fake: &Fake, own: i32) -> Option<Front> {
        let (front, captured) = read_with(fake, own, || ("captured", Some(CapturedWindow::Macos(1))));
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
    fn site_comes_from_the_captured_window_after_a_tab_switch_or_handle_reuse() {
        for (system, id, window) in [
            (System::Macos, "com.google.Chrome", CapturedWindow::Macos(1)),
            (System::Windows, "chrome.exe", CapturedWindow::Windows(1)),
        ] {
            let fake = Fake {
                window,
                ..Fake::new(system, id, Some("https://overleaf.com/"))
            };
            let mut captures = 0;
            let (front, captured) = read_with(&fake, 7, || {
                captures += 1;
                // Ctrl+Tab changes the site without changing the window. A
                // replacement window reusing an HWND must also get its own site.
                fake.address.set(Some("https://docs.google.com/document/d/invented"));
                ("captured", Some(window))
            });
            assert_eq!(captures, 1);
            assert_eq!(captured, "captured");
            assert_eq!(site_of(front), site("https://docs.google.com/"));
        }
    }

    #[test]
    fn site_is_read_from_the_retained_window_even_if_another_window_has_focus() {
        let fake = Fake {
            window: CapturedWindow::Macos(2),
            ..Fake::new(System::Macos, "com.google.Chrome", Some("https://overleaf.com/"))
        };
        let (front, _) = read_with(&fake, 7, || ((), Some(CapturedWindow::Macos(2))));
        assert_eq!(site_of(front), site("https://overleaf.com/"));
        assert_eq!(site_of(read_with(&fake, 7, || ((), None)).0), None);
    }

    #[test]
    fn a_late_site_read_leaves_the_browser_format_in_charge() {
        let fake = Fake {
            slow: true,
            ..Fake::new(System::Macos, "com.google.Chrome", Some("https://overleaf.com/"))
        };
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
