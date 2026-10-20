//! Feedback links in the menu. They open the GitHub forms in the person's own
//! browser; Ibis itself sends nothing.

use std::process::Command;

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
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

/// The idea form.
pub(crate) fn idea_url() -> String {
    format!("{ISSUES}?template=feature_request.yml")
}

/// Open `url` in the default browser. If that fails, copy it and say so.
pub(crate) fn open_in_browser(app: &AppHandle, url: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        let opened = browser_command(&url).status().is_ok_and(|status| status.success());
        if !opened {
            let copied = app.clipboard().write_text(url.clone()).is_ok();
            let text = if copied {
                "Ibis could not open your browser. The feedback link was copied: paste it into your browser."
                    .to_string()
            } else {
                format!("Ibis could not open your browser. Open this link to send feedback:\n{url}")
            };
            app.dialog().message(text).title("Send feedback").show(|_| {});
        }
    });
}

fn browser_command(url: &str) -> Command {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    // `start` through cmd would treat `&` in the link as a command separator.
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    command.arg(url);
    command
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
            "https://github.com/ibiscite/ibis/issues/new?template=feature_request.yml"
        );
    }
}
