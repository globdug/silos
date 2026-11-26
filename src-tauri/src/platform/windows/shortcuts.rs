//! Windows-only half of shortcut creation: rendering the `.ico` and writing
//! the actual `.lnk` file into the Start Menu. OS-agnostic parts (icon pixel
//! pipeline, filename sanitizing, cache-busting path) live in the common
//! `crate::shortcuts` facade, which this module is called from.

use mslnk::ShellLink;
use std::path::{Path, PathBuf};

/// Per-user Start Menu folder for Silos's webapp shortcuts. No admin
/// rights and no registry entries needed (unlike an NSIS-installed app).
fn start_menu_dir() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(
        PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Silos"),
    )
}

fn shortcut_path(app_name: &str) -> Option<PathBuf> {
    Some(start_menu_dir()?.join(format!("{}.lnk", crate::shortcuts::sanitize_filename(app_name))))
}

/// Builds a Start Menu-compatible `.ico` from whatever image format the
/// user picked (or a fetched favicon.ico). Returns the resulting path, or
/// `None` if it couldn't be produced (unsupported/corrupt source), in which
/// case the caller falls back to the exe's own icon.
fn build_icon(source: &str, app_id: &str, background: Option<&str>, padding_percent: u8, rounded: bool) -> Option<PathBuf> {
    let source_bytes = std::fs::read(source).ok()?;
    let hash = crate::shortcuts::content_hash(&[
        &source_bytes,
        background.unwrap_or("").as_bytes(),
        &[padding_percent, rounded as u8],
    ]);
    let dest = crate::shortcuts::generated_icon_path(app_id, "ico", &hash);
    std::fs::create_dir_all(dest.parent()?).ok()?;
    crate::shortcuts::remove_old_shortcut_icons(app_id, &dest);

    // An already-.ico source is only left byte-for-byte as-is (skipping the
    // rounded mask below) when the app's icon style doesn't need rounding —
    // re-rasterizing here would collapse it to a single size, so an .ico
    // asked to be rounded still goes through the same pipeline as everything
    // else instead of taking this shortcut (pun intended).
    let is_ico = Path::new(source)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("ico"))
        .unwrap_or(false);
    if is_ico && !rounded {
        std::fs::copy(source, &dest).ok()?;
        return Some(dest);
    }

    let resized = crate::shortcuts::render_icon(source, background, padding_percent, rounded)?;
    let icon_image = ico::IconImage::from_rgba_data(256, 256, resized.into_raw());
    let entry = ico::IconDirEntry::encode(&icon_image).ok()?;
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    icon_dir.add_entry(entry);

    let file = std::fs::File::create(&dest).ok()?;
    icon_dir.write(file).ok()?;
    Some(dest)
}

/// Creates (or overwrites) a Start Menu shortcut that launches this webapp
/// directly, bypassing the dashboard: `silos.exe --open-app <id>`.
/// `data_slug` must match the one used for the webapp window's own
/// AppUserModelID (see `super::winid::aumid_for_slug` callers) — stamping
/// the same ID here is what lets toast notifications for this app be shown
/// at all (an unregistered AUMID is rejected by
/// `ToastNotificationManager::CreateToastNotifierWithId`).
///
/// Returns whether the `.lnk` was actually written.
pub fn create_shortcut(
    app_id: &str,
    data_slug: &str,
    app_name: &str,
    icon_source: Option<&str>,
    icon_background: Option<&str>,
    icon_padding_percent: u8,
    icon_rounded: bool,
) -> bool {
    let Some(dir) = start_menu_dir() else { return false };
    let Some(path) = shortcut_path(app_name) else { return false };
    let Ok(exe) = std::env::current_exe() else { return false };

    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }

    let Ok(mut link) = ShellLink::new(&exe) else { return false };
    link.set_name(Some(app_name.to_string()));
    link.set_arguments(Some(format!("--open-app {app_id}")));

    let icon_path = icon_source.and_then(|src| build_icon(src, app_id, icon_background, icon_padding_percent, icon_rounded));
    let icon = icon_path
        .as_deref()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .or_else(|| exe.to_str().map(|s| s.to_string()));
    link.set_icon_location(icon);

    if link.create_lnk(&path).is_err() {
        return false;
    }
    super::winid::set_shortcut_app_id(&path, &super::winid::aumid_for_slug("Silos.WebApp", data_slug));
    true
}

pub fn remove_shortcut(_app_id: &str, app_name: &str) {
    if let Some(path) = shortcut_path(app_name) {
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }
}
