//! Linux: Appearance's System follows the desktop's dark preference (decision
//! 0041). WebKitGTK reads `prefers-color-scheme` from GTK's prefer-dark
//! setting, which neither the desktop portal's colour scheme (GNOME, KDE) nor a
//! dark GTK theme variant (`GTK_THEME=Adwaita:dark`) changes, so Ibis sets it
//! once it has read them. Light and Dark in Appearance still win in the page.

/// Whether the desktop asks for dark: the portal's colour scheme when it
/// answers, else a GTK theme whose variant or name says dark.
#[cfg(any(target_os = "linux", test))]
fn prefers_dark(portal: Option<bool>, gtk_theme: Option<&str>, theme_name: Option<&str>) -> bool {
    let dark = |name: &str| {
        let name = name.trim().to_ascii_lowercase();
        name.ends_with(":dark") || name.ends_with("-dark")
    };
    portal.unwrap_or(false) || gtk_theme.is_some_and(dark) || theme_name.is_some_and(dark)
}

/// Read at start, off the main thread and bounded, so a missing portal never
/// delays the first window.
#[cfg(target_os = "linux")]
pub(crate) fn follow(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let portal = portal_prefers_dark().await;
        let handle = app.clone();
        crate::diagnostics::report(
            app.run_on_main_thread(move || {
                let theme = std::env::var("GTK_THEME").ok();
                let name = gtk::Settings::default()
                    .and_then(|settings| gtk::prelude::GtkSettingsExt::gtk_theme_name(&settings))
                    .map(|name| name.to_string());
                if prefers_dark(portal, theme.as_deref(), name.as_deref()) {
                    handle.set_theme(Some(tauri::Theme::Dark));
                }
            }),
            crate::diagnostics::NativeIssue::Window,
        );
    });
}

/// `org.freedesktop.appearance color-scheme`: 1 prefers dark, 0 and 2 do not;
/// no answer within the call limit is unknown.
#[cfg(target_os = "linux")]
async fn portal_prefers_dark() -> Option<bool> {
    let call = async {
        let connection = zbus::Connection::session().await.ok()?;
        let reply = connection
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Settings"),
                "ReadOne",
                &("org.freedesktop.appearance", "color-scheme"),
            )
            .await
            .ok()?;
        let value: zbus::zvariant::OwnedValue = reply.body().deserialize().ok()?;
        let scheme = match &*value {
            zbus::zvariant::Value::Value(inner) => u32::try_from(&**inner).ok(),
            other => u32::try_from(other).ok(),
        }?;
        Some(scheme == 1)
    };
    tokio::time::timeout(std::time::Duration::from_secs(2), call)
        .await
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::prefers_dark;

    #[test]
    fn system_follows_the_desktops_dark_preference() {
        assert!(
            prefers_dark(Some(true), None, Some("Adwaita")),
            "the portal asks for dark"
        );
        assert!(
            prefers_dark(None, Some("Adwaita:dark"), Some("Adwaita")),
            "GTK_THEME's dark variant"
        );
        assert!(prefers_dark(None, None, Some("Breeze-Dark")), "a dark theme by name");
        assert!(
            !prefers_dark(Some(false), None, Some("Adwaita")),
            "a light desktop stays light"
        );
        assert!(
            !prefers_dark(None, Some("Adwaita"), Some("Darkroom")),
            "only a dark variant counts"
        );
        assert!(!prefers_dark(None, None, None), "nothing known stays light");
    }
}
