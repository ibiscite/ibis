//! Feedback links in the menu. They open the GitHub forms in the person's own
//! browser; Ibis itself sends nothing.

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

const ISSUES: &str = "https://github.com/ibiscite/ibis/issues/new";

/// The problem form, with the version and operating system filled in. Both
/// are fixed at build time; a version is semver, so only letters, digits,
/// `.`, `-` and `+` need checking, and `+` is escaped.
pub(crate) fn problem_url(version: &str) -> String {
    let version: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || ".-+".contains(*c))
        .collect();
    format!(
        "{ISSUES}?template=bug_report.yml&version={}&os={}%20{}",
        version.replace('+', "%2B"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// The Ideas form in Discussions, where ideas start (CONTRIBUTING.md).
pub(crate) fn idea_url() -> String {
    "https://github.com/ibiscite/ibis/discussions/new?category=ideas".to_owned()
}

/// A fixed page linked from About. The web page never chooses the address.
#[derive(Clone, Copy, Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AboutLink {
    Website,
    Source,
    Problem,
    Licences,
}

#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn open_about_link(app: AppHandle, link: AboutLink) {
    let url = match link {
        AboutLink::Website => "https://ibiscite.org".to_owned(),
        AboutLink::Source => "https://github.com/ibiscite/ibis".to_owned(),
        AboutLink::Problem => problem_url(&app.package_info().version.to_string()),
        AboutLink::Licences => "https://github.com/ibiscite/ibis/blob/main/LICENSE".to_owned(),
    };
    open_in_browser(&app, url);
}

/// Open `url` in the default browser. If that fails, copy it and say so.
pub(crate) fn open_in_browser(app: &AppHandle, url: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        let opened = crate::system_open::open(url.as_ref(), false).is_ok();
        if !opened {
            let copied = crate::copy::write_plain(&app, &url).is_ok();
            let say = |id: &str| ibis_engine::catalogue::text(id, None, &[]).unwrap_or_default();
            let text = if copied {
                say("app-open-link-copied")
            } else {
                format!("{}\n{url}", say("app-open-link-failed"))
            };
            app.dialog()
                .message(text)
                .title(say("app-open-link-title"))
                .show(|_| {});
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_links_name_the_forms_and_carry_only_safe_characters() {
        let problem = problem_url("0.1.1-nightly.202610192230+build&x=1");
        assert!(problem.contains("&version=0.1.1-nightly.202610192230%2Bbuildx1&"));
        assert!(problem.starts_with("https://github.com/ibiscite/ibis/issues/new?template=bug_report.yml&version="));
        assert!(problem.contains(&format!("&os={}%20", std::env::consts::OS)));
        let query = problem.split_once('?').map_or("", |(_, query)| query);
        assert!(!query.is_empty());
        assert!(query.chars().all(|c| c.is_ascii_alphanumeric() || "=&._-%".contains(c)));
        assert_eq!(
            idea_url(),
            "https://github.com/ibiscite/ibis/discussions/new?category=ideas"
        );
    }
}
