//! System-owned app/file icons only. Failure is optional presentation, never
//! a reason to disable opening the source. No document thumbnails are exposed;
//! icons never poll.

use super::SystemIcon;
use std::path::PathBuf;

const MAX_ICON_BYTES: usize = 64 * 1024;

fn bounded(mime: &str, bytes: Vec<u8>) -> Option<SystemIcon> {
    if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES || !matches!(mime, "image/png" | "image/jpeg") {
        return None;
    }
    Some(SystemIcon {
        mime: mime.into(),
        bytes,
    })
}

#[cfg(not(target_os = "windows"))]
pub(super) async fn load(app: tauri::AppHandle, path: Option<PathBuf>) -> Option<SystemIcon> {
    // AppKit and GTK icon APIs belong to the UI thread. Bound waiting for it;
    // an unavailable/closing desktop simply uses the shared fallback drawing.
    tauri::async_runtime::spawn_blocking(move || {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let _ = send.send(platform(path));
        })
        .ok()?;
        let raw = receive
            .recv_timeout(std::time::Duration::from_millis(500))
            .ok()
            .flatten()?;
        normalize(raw)
    })
    .await
    .ok()
    .flatten()
}

#[cfg(target_os = "macos")]
fn platform(path: Option<PathBuf>) -> Option<SystemIcon> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;
    let workspace = NSWorkspace::sharedWorkspace();
    let path = match path {
        Some(path) => NSString::from_str(path.to_str()?),
        None => workspace
            .URLForApplicationWithBundleIdentifier(&NSString::from_str("org.zotero.zotero"))?
            .path()?,
    };
    let data = workspace.iconForFile(&path).TIFFRepresentation()?;
    if data.len() > 8 * 1024 * 1024 {
        return None;
    }
    Some(SystemIcon {
        mime: "image/tiff".into(),
        bytes: data.to_vec(),
    })
}

#[cfg(target_os = "macos")]
fn normalize(raw: SystemIcon) -> Option<SystemIcon> {
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(raw.bytes), image::ImageFormat::Tiff);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let icon = reader.decode().ok()?.thumbnail(32, 32);
    let mut png = std::io::Cursor::new(Vec::new());
    icon.write_to(&mut png, image::ImageFormat::Png).ok()?;
    bounded("image/png", png.into_inner())
}

#[cfg(target_os = "linux")]
fn platform(path: Option<PathBuf>) -> Option<SystemIcon> {
    use gtk::{gio, prelude::*};
    // Zotero artwork comes from the installed Mac app only; other systems
    // retain Ibis's books glyph. File icons still come from the system.
    let icon = gio::File::for_path(path?)
        .query_info("standard::icon", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
        .ok()?
        .icon()?;
    let pixels = gtk::IconTheme::default()?
        .lookup_by_gicon(&icon, 32, gtk::IconLookupFlags::FORCE_SIZE)?
        .load_icon()
        .ok()?;
    bounded("image/png", pixels.save_to_bufferv("png", &[]).ok()?)
}

#[cfg(target_os = "linux")]
fn normalize(raw: SystemIcon) -> Option<SystemIcon> {
    Some(raw)
}

#[cfg(target_os = "windows")]
pub(super) async fn load(_app: tauri::AppHandle, path: Option<PathBuf>) -> Option<SystemIcon> {
    tauri::async_runtime::spawn_blocking(move || platform_windows(path))
        .await
        .ok()
        .flatten()
}

#[cfg(target_os = "windows")]
fn platform_windows(path: Option<PathBuf>) -> Option<SystemIcon> {
    use windows::{
        Storage::{
            FileProperties::{ThumbnailMode, ThumbnailType},
            StorageFile,
            Streams::DataReader,
        },
        core::HSTRING,
    };
    // File icons only. Zotero keeps the shared books glyph on Windows.
    let (mime, size, input) = {
        let path = path?;
        let canonical = super::source_file(&path).ok()?;
        let path = dunce::simplified(&canonical);
        let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.to_str()?))
            .ok()?
            .join()
            .ok()?;
        let icon = file
            .GetThumbnailAsyncOverloadDefaultOptions(ThumbnailMode::ListView, 32)
            .ok()?
            .join()
            .ok()?;
        if icon.Type().ok()? != ThumbnailType::Icon {
            return None;
        }
        (
            icon.ContentType().ok()?.to_string(),
            icon.Size().ok()?,
            icon.GetInputStreamAt(0).ok()?,
        )
    };
    if size == 0 || size > MAX_ICON_BYTES as u64 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&input).ok()?;
    if reader.LoadAsync(size as u32).ok()?.join().ok()? != size as u32 {
        return None;
    }
    let mut bytes = vec![0; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    bounded(&mime, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_or_unbounded_icons_use_the_fallback_without_payloads() {
        assert!(bounded("image/png", vec![]).is_none());
        assert!(bounded("image/png", vec![0; MAX_ICON_BYTES + 1]).is_none());
        assert!(bounded("image/svg+xml", b"<svg/>".to_vec()).is_none());
        assert!(bounded("image/png", vec![137, 80, 78, 71]).is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn runtime_raster_conversion_bounds_size_and_rejects_invalid_or_oversized_images() {
        let tiff = |width, height| {
            let image = image::DynamicImage::new_rgba8(width, height);
            let mut data = std::io::Cursor::new(Vec::new());
            image
                .write_to(&mut data, image::ImageFormat::Tiff)
                .unwrap_or_else(|e| panic!("{e}"));
            SystemIcon {
                mime: "image/tiff".into(),
                bytes: data.into_inner(),
            }
        };
        let icon = normalize(tiff(64, 32)).unwrap_or_else(|| panic!("invented system raster"));
        assert_eq!(icon.mime, "image/png");
        assert!(icon.bytes.len() <= MAX_ICON_BYTES);
        let image =
            image::load_from_memory_with_format(&icon.bytes, image::ImageFormat::Png).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((image.width(), image.height()), (32, 16));
        assert!(normalize(tiff(3000, 1)).is_none());
        assert!(
            normalize(SystemIcon {
                mime: "image/tiff".into(),
                bytes: b"invented invalid image".to_vec()
            })
            .is_none()
        );
    }
}
