use crate::downloads::DownloadRegistry;
use crate::layout::{
    add_popup_label, app_settings_window_label, app_window_label, downloads_popup_label, lock_overlay_label,
    quit_confirm_label, settings_window_label, sidebar_label, subspace_key, tab_label,
    tab_prefix_for_app, tab_prefix_for_subspace, toolbar_label, toolbar_prefix, ActiveSubspaces,
    BackgroundApps, LockedApps, SidebarWidths, SubspaceTabs, TabInfo, TabRegistry,
    GLOBAL_SETTINGS_LABEL, SIDEBAR_WIDTH_COLLAPSED, SIDEBAR_WIDTH_EXPANDED, TOOLBAR_HEIGHT,
};
use crate::models::{default_icon_background, IconStyle, Subspace, WebApp, WebAppView};
use crate::store::Store;
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::webview::WebviewBuilder;
use tauri::window::WindowBuilder;
use tauri::window::Window;
use tauri::utils::config::Color;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, WebviewUrl};
use serde::{Deserialize, Serialize};

const APP_BG: Color = Color(18, 18, 20, 255);

/// A bare reqwest client sends no User-Agent at all, which Wikimedia (and a
/// lot of sites behind bot-protection/WAFs) reject outright with a generic
/// "Your request has been blocked" page instead of the real content. Every
/// outbound fetch in this module goes through this client instead of the
/// bare `reqwest::get` shorthand, identifying as an ordinary desktop browser.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Silos/0.1")
        .build()
        .expect("failed to build http client")
}

/// Fake URL scheme the injected click-intercept script redirects to instead
/// of letting a ctrl/shift/middle-clicked link navigate normally. Caught in
/// on_navigation below and turned into a new tab, then blocked so the
/// current tab never actually navigates to it.
const NEW_TAB_SCHEME: &str = "silos-newtab";

const NEW_TAB_INTERCEPT_SCRIPT: &str = r#"
(function () {
  function findLink(el) {
    while (el && el !== document.body) {
      if (el.tagName === 'A' && el.href) return el;
      el = el.parentElement;
    }
    return null;
  }
  function signal(url) {
    location.href = 'silos-newtab://open?url=' + encodeURIComponent(url);
  }
  document.addEventListener('click', function (e) {
    if (e.button !== 0 || !(e.ctrlKey || e.metaKey || e.shiftKey)) return;
    var a = findLink(e.target);
    if (!a) return;
    e.preventDefault();
    e.stopPropagation();
    signal(a.href);
  }, true);
  document.addEventListener('auxclick', function (e) {
    if (e.button !== 1) return;
    var a = findLink(e.target);
    if (!a) return;
    e.preventDefault();
    e.stopPropagation();
    signal(a.href);
  }, true);
})();
"#;

/// Injected into every tab so `platform::webview::setup_zoom_indicator`'s
/// native `ZoomFactorChanged` hook has something to call: WebView2's
/// ctrl+scroll/ctrl+`+`/`-` zoom (from `.zoom_hotkeys_enabled(true)` below)
/// is handled entirely inside the browser engine with no JS-visible event,
/// so the indicator has to be driven from the Rust side via `eval` instead
/// of a page-side listener. Lives in the page's own viewport (not a
/// separate OS window) so it tracks the zoomed content exactly like a real
/// browser's zoom HUD would.
const ZOOM_HUD_SCRIPT: &str = r#"
(function () {
  var el = null;
  var hideTimer = null;
  window.__silosZoomHud = function (pct) {
    if (!el) {
      el = document.createElement('div');
      el.style.cssText = 'position:fixed;right:16px;bottom:16px;z-index:2147483647;' +
        'background:rgba(20,20,20,.85);color:#fff;font:600 13px system-ui,sans-serif;' +
        'padding:6px 10px;border-radius:6px;pointer-events:none;' +
        'transition:opacity .15s ease;box-shadow:0 2px 8px rgba(0,0,0,.3);opacity:0;';
      (document.body || document.documentElement).appendChild(el);
    }
    el.textContent = pct + '%';
    el.style.opacity = '1';
    clearTimeout(hideTimer);
    hideTimer = setTimeout(function () { el.style.opacity = '0'; }, 900);
  };
})();
"#;

/// Only allow characters that can never escape a single path component
/// (no `/`, `\`, `..`, NUL, etc.) — `session_group`/`app_slug` end up joined
/// straight into a filesystem path that later gets `remove_dir_all`'d, so a
/// crafted value must not be able to point outside `webapps_dir()`.
fn is_safe_path_component(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn session_data_dir(app_slug: &str, session_group: &str) -> std::path::PathBuf {
    crate::store::webapps_dir().join(app_slug).join(session_group)
}

/// Defense in depth for callers that then recursively delete the returned
/// directory: refuses to hand back a path unless both components are safe
/// AND the resolved path is actually still inside `webapps_dir()`.
fn safe_session_data_dir(app_slug: &str, session_group: &str) -> Result<std::path::PathBuf, String> {
    if !is_safe_path_component(app_slug) || !is_safe_path_component(session_group) {
        return Err("invalid session path".into());
    }
    let expected_parent = crate::store::webapps_dir().join(app_slug);
    let dir = session_data_dir(app_slug, session_group);
    if dir.parent() != Some(expected_parent.as_path()) {
        return Err("invalid session path".into());
    }
    Ok(dir)
}

fn icons_dir(_app: &AppHandle) -> std::path::PathBuf {
    crate::store::data_root().join("icons")
}

/// Tauri's window/tray icon API takes one flat RGBA bitmap (no multi-size
/// .ico support), which the OS then stretches to whatever it actually needs
/// (16-24px in the tray). Handing it a huge source (macOS-style icons are
/// often 512-1024px) makes that final shrink very lossy. Pre-shrinking here
/// with a real resampling filter to a size close to what's actually
/// rendered gives Windows' own last-mile scaling much less work to ruin.
const ICON_RENDER_SIZE: u32 = 64;

/// Rounds an icon bitmap's corners in place to match the app's icon_style
/// (CSS `border-radius` only affects the in-app `<img>`, never the native
/// Windows window/taskbar/tray icon — has to be baked into the pixels here).
/// Corner pixels get their alpha scaled by how far outside the rounded-rect
/// they fall, with ~1px of antialiasing at the boundary.
pub(crate) fn apply_rounded_mask(mut rgba: image::RgbaImage) -> image::RgbaImage {
    const RADIUS_RATIO: f32 = 0.22;

    let (w, h) = rgba.dimensions();
    let (wf, hf) = (w as f32, h as f32);
    let radius = (wf.min(hf) * RADIUS_RATIO).max(1.0);

    for y in 0..h {
        for x in 0..w {
            let (xf, yf) = (x as f32, y as f32);
            let in_corner_x = xf < radius || xf > wf - radius;
            let in_corner_y = yf < radius || yf > hf - radius;
            if !(in_corner_x && in_corner_y) {
                continue;
            }
            let cx = if xf < radius { radius } else { wf - radius };
            let cy = if yf < radius { radius } else { hf - radius };
            let dist = ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt();
            let coverage = (radius - dist + 0.5).clamp(0.0, 1.0);
            let pixel = rgba.get_pixel_mut(x, y);
            pixel[3] = (pixel[3] as f32 * coverage).round() as u8;
        }
    }
    rgba
}

/// Parses a `#rrggbb` hex string into RGB components. `None` for anything
/// else (missing, `#rgb` shorthand, invalid digits) — callers treat that as
/// "leave transparent".
pub(crate) fn parse_hex_color(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b])
}

/// Flattens a transparent/semi-transparent icon onto a solid color, since the
/// app's `icon_background_color` is only ever painted by CSS behind the
/// in-app `<img>` — the native window/taskbar/tray/shortcut icon is handed a
/// raw bitmap with no OS-side "background" concept, so a transparent source
/// (a trimmed PNG logo, say) renders as blank/invisible chrome there unless
/// the color is baked into the pixels here too.
pub(crate) fn composite_on_background(mut rgba: image::RgbaImage, hex: &str) -> image::RgbaImage {
    let Some([br, bg, bb]) = parse_hex_color(hex) else {
        return rgba;
    };
    for pixel in rgba.pixels_mut() {
        let a = pixel[3] as f32 / 255.0;
        pixel[0] = (pixel[0] as f32 * a + br as f32 * (1.0 - a)).round() as u8;
        pixel[1] = (pixel[1] as f32 * a + bg as f32 * (1.0 - a)).round() as u8;
        pixel[2] = (pixel[2] as f32 * a + bb as f32 * (1.0 - a)).round() as u8;
        pixel[3] = 255;
    }
    rgba
}

/// Resizes to a `size`x`size` square with an inset border, so the source
/// image doesn't touch the edges. `padding_percent` (0-40, clamped) shrinks
/// the resized image and centers it on a fully transparent canvas of `size`;
/// the empty border is left for `composite_on_background`/the OS to fill.
pub(crate) fn pad_and_resize(img: &image::DynamicImage, size: u32, padding_percent: u8) -> image::RgbaImage {
    let padding_percent = padding_percent.min(40);
    let inner = ((size as f32) * (1.0 - 2.0 * padding_percent as f32 / 100.0)).round().max(1.0) as u32;
    let resized = img.resize_exact(inner, inner, image::imageops::FilterType::Lanczos3).to_rgba8();
    if inner == size {
        return resized;
    }
    let mut canvas = image::RgbaImage::new(size, size);
    let offset = ((size - inner) / 2) as i64;
    image::imageops::overlay(&mut canvas, &resized, offset, offset);
    canvas
}

/// Decodes any image format the `image` crate supports into the RGBA buffer
/// Tauri's window icon API wants. SVG isn't supported (no rasterizer here).
fn decode_icon_bytes(
    bytes: &[u8],
    rounded: bool,
    background: Option<&str>,
    padding_percent: u8,
) -> Option<tauri::image::Image<'static>> {
    let img = image::load_from_memory(bytes).ok()?;
    let mut rgba = pad_and_resize(&img, ICON_RENDER_SIZE, padding_percent);
    if let Some(hex) = background {
        rgba = composite_on_background(rgba, hex);
    }
    if rounded {
        rgba = apply_rounded_mask(rgba);
    }
    let (width, height) = rgba.dimensions();
    Some(tauri::image::Image::new_owned(rgba.into_raw(), width, height))
}

fn load_icon_image(path: &str, rounded: bool, background: Option<&str>, padding_percent: u8) -> Option<tauri::image::Image<'static>> {
    decode_icon_bytes(&std::fs::read(path).ok()?, rounded, background, padding_percent)
}

/// Baked into the binary (not read from disk, since a portable build has no
/// `icons/` folder next to the exe) so there's always a valid icon to fall
/// back to for apps that don't have their own yet.
const DEFAULT_ICON_BYTES: &[u8] = include_bytes!("../icons/icon.ico");

pub(crate) fn default_icon_image() -> Option<tauri::image::Image<'static>> {
    decode_icon_bytes(DEFAULT_ICON_BYTES, false, None, 0)
}

/// Resolves an app's chosen icon if it has and can decode one, otherwise the
/// bundled default. Explicit either way — window icon on Windows isn't
/// reliably inherited from the exe resource for windows built without one.
/// `background` is the app's `icon_background_color`, baked into the pixels
/// so a transparent source icon doesn't render blank in taskbar/tray.
fn resolve_window_icon(
    icon_path: Option<&str>,
    rounded: bool,
    background: Option<&str>,
    padding_percent: u8,
) -> Option<tauri::image::Image<'static>> {
    icon_path
        .and_then(|p| load_icon_image(p, rounded, background, padding_percent))
        .or_else(default_icon_image)
}

/// Applies an app's icon to its already-open window's titlebar/taskbar icon
/// (if the window is open), so favicon/pick-image changes made from settings
/// show up immediately, not just on next launch.
fn apply_window_icon(
    app_handle: &AppHandle,
    app_id: &str,
    icon_path: Option<&str>,
    rounded: bool,
    background: Option<&str>,
    padding_percent: u8,
) {
    let Some(window) = app_handle.get_window(&app_window_label(app_id)) else { return };
    if let Some(icon) = resolve_window_icon(icon_path, rounded, background, padding_percent) {
        let _ = window.set_icon(icon);
    }
}

fn notify_sidebar(app_handle: &AppHandle, app_id: &str) {
    let _ = app_handle.emit_to(sidebar_label(app_id), "app-data-changed", ());
}

fn notify_toolbar(app_handle: &AppHandle, app_id: &str, subspace_id: &str) {
    let _ = app_handle.emit_to(toolbar_label(app_id, subspace_id), "tabs-changed", ());
}

fn mime_for_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn content_geometry(window: &Window, sidebar_width: f64) -> Result<(f64, f64), String> {
    let logical = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
    Ok(((logical.width - sidebar_width).max(0.0), logical.height))
}

#[tauri::command]
pub fn list_apps(store: State<Store>) -> Vec<WebAppView> {
    store.config.lock().unwrap().apps.iter().map(WebAppView::from).collect()
}

#[derive(serde::Serialize)]
pub struct SubspaceSize {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
}

#[derive(serde::Serialize)]
pub struct AppInfo {
    pub created_at: i64,
    pub folder_path: String,
    pub total_size_bytes: u64,
    pub subspaces: Vec<SubspaceSize>,
}

fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries.flatten().fold(0u64, |total, entry| {
        let Ok(metadata) = entry.metadata() else { return total };
        total + if metadata.is_dir() { dir_size(&entry.path()) } else { metadata.len() }
    })
}

/// "Informazioni" panel data for an app's settings: on-disk location and
/// size, computed on demand (not cached) since it's only requested when that
/// settings section is opened.
#[tauri::command]
pub fn get_app_info(store: State<Store>, app_id: String) -> Result<AppInfo, String> {
    let config = store.config.lock().unwrap();
    let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
    let app_dir = crate::store::webapps_dir().join(&app.data_slug);

    // Subspaces sharing a session_group share the same folder: size it once
    // per distinct group, not once per subspace.
    let mut group_sizes: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let subspaces = app
        .subspaces
        .iter()
        .map(|s| {
            let size = *group_sizes
                .entry(s.session_group.clone())
                .or_insert_with(|| dir_size(&app_dir.join(&s.session_group)));
            SubspaceSize { id: s.id.clone(), name: s.name.clone(), size_bytes: size }
        })
        .collect();

    Ok(AppInfo {
        created_at: app.created_at,
        folder_path: app_dir.to_string_lossy().to_string(),
        total_size_bytes: dir_size(&app_dir),
        subspaces,
    })
}

#[tauri::command]
pub fn create_app(
    store: State<Store>,
    name: String,
    url: String,
    icon: Option<String>,
    icon_fit: Option<String>,
    icon_rounded: Option<bool>,
    icon_background_color: Option<String>,
    icon_padding_percent: Option<u8>,
) -> WebAppView {
    let mut config = store.config.lock().unwrap();
    let taken: std::collections::HashSet<String> = config.apps.iter().map(|a| a.data_slug.clone()).collect();
    let data_slug = crate::store::slugify(&name, &taken);

    let default_style = IconStyle::default();
    let app_id = uuid::Uuid::new_v4().to_string();
    let default_subspace_id = uuid::Uuid::new_v4().to_string();
    let default_subspace = Subspace {
        session_group: default_subspace_id.clone(),
        id: default_subspace_id,
        name: name.clone(),
        icon: icon.clone(),
        icon_background_color: icon_background_color.clone().or_else(default_icon_background),
        start_url: Some(url.clone()),
    };
    let app = WebApp {
        id: app_id,
        name,
        url,
        icon,
        icon_style: IconStyle {
            fit: icon_fit.unwrap_or(default_style.fit),
            rounded: icon_rounded.unwrap_or(default_style.rounded),
            padding_percent: icon_padding_percent.unwrap_or(default_style.padding_percent),
        },
        icon_background_color,
        data_slug,
        run_in_background: false,
        eager_load_subspaces: true,
        hibernate_delay_secs: 300,
        pin_hash: None,
        pin_lock_on_background: false,
        pin_lock_delay_secs: 0,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        ignore_certificate_errors: false,
        download_mode: crate::models::default_download_mode(),
        subspaces: vec![default_subspace],
    };
    config.apps.push(app.clone());
    drop(config);
    store.save();
    crate::shortcuts::create_shortcut(
        &app.id,
        &app.data_slug,
        &app.name,
        app.icon.as_deref(),
        app.icon_background_color.as_deref(),
        app.icon_style.padding_percent,
        app.icon_style.rounded,
    );
    WebAppView::from(&app)
}

#[tauri::command]
pub fn delete_app(app_handle: AppHandle, store: State<Store>, app_id: String) {
    // Removed from the config *before* closing the window: the close handler
    // checks run_in_background live from the store, and would otherwise just
    // hide a backgrounded app's window into the tray instead of closing it.
    let mut config = store.config.lock().unwrap();
    let removed = config.apps.iter().find(|a| a.id == app_id).cloned();
    config.apps.retain(|a| a.id != app_id);
    drop(config);
    store.save();

    if let Some(window) = app_handle.get_window(&app_window_label(&app_id)) {
        let _ = window.close();
    }

    let background = app_handle.state::<BackgroundApps>();
    background.0.lock().unwrap().remove(&app_id);
    let _ = app_handle.remove_tray_by_id(&crate::layout::tray_id_for_app(&app_id));

    if let Some(app) = removed {
        crate::shortcuts::remove_shortcut(&app.id, &app.name);
        crate::shortcuts::remove_all_shortcut_icons(&app.id);
        let dir = crate::store::webapps_dir().join(&app.data_slug);
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

#[tauri::command]
pub fn create_subspace(
    app_handle: AppHandle,
    store: State<Store>,
    app_id: String,
    name: String,
    start_url: Option<String>,
    icon: Option<String>,
    session_group: Option<String>,
    icon_background_color: Option<String>,
) -> Option<Subspace> {
    let id = uuid::Uuid::new_v4().to_string();
    let subspace = Subspace {
        session_group: session_group.filter(|g| !g.trim().is_empty()).unwrap_or_else(|| id.clone()),
        id: id.clone(),
        name,
        icon,
        icon_background_color: icon_background_color.or_else(default_icon_background),
        start_url: start_url.filter(|u| !u.trim().is_empty()),
    };

    let mut config = store.config.lock().unwrap();
    let app = config.apps.iter_mut().find(|a| a.id == app_id)?;
    app.subspaces.push(subspace.clone());
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
    Some(subspace)
}

#[derive(Serialize, Clone)]
pub struct SessionGroupInfo {
    pub group: String,
    pub label: String,
}

/// Lists this app's existing session groups (deduplicated, with the names of
/// the subspaces sharing each one) so a new subspace can join one instead of
/// always getting an isolated session.
#[tauri::command]
pub fn list_session_groups(store: State<Store>, app_id: String) -> Vec<SessionGroupInfo> {
    let config = store.config.lock().unwrap();
    let Some(app) = config.apps.iter().find(|a| a.id == app_id) else {
        return vec![];
    };

    let mut groups: Vec<(String, Vec<String>)> = vec![];
    for s in &app.subspaces {
        if let Some(entry) = groups.iter_mut().find(|(g, _)| g == &s.session_group) {
            entry.1.push(s.name.clone());
        } else {
            groups.push((s.session_group.clone(), vec![s.name.clone()]));
        }
    }

    groups
        .into_iter()
        .map(|(group, names)| SessionGroupInfo { group, label: names.join(", ") })
        .collect()
}

/// Closes the toolbar and every open tab for a subspace.
fn close_subspace_views(app_handle: &AppHandle, registry: &TabRegistry, app_id: &str, subspace_id: &str) {
    if let Some(webview) = app_handle.get_webview(&toolbar_label(app_id, subspace_id)) {
        let _ = webview.close();
    }
    let prefix = tab_prefix_for_subspace(app_id, subspace_id);
    for (label, webview) in app_handle.webviews() {
        if label.starts_with(&prefix) {
            let _ = webview.close();
        }
    }
    registry.0.lock().unwrap().remove(&subspace_key(app_id, subspace_id));
}

#[tauri::command]
pub fn delete_subspace(
    app_handle: AppHandle,
    store: State<Store>,
    registry: State<TabRegistry>,
    app_id: String,
    subspace_id: String,
) {
    close_subspace_views(&app_handle, &registry, &app_id, &subspace_id);

    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        app.subspaces.retain(|s| s.id != subspace_id);
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
}

#[tauri::command]
pub fn set_subspace_session_group(
    app_handle: AppHandle,
    store: State<Store>,
    app_id: String,
    subspace_id: String,
    session_group: String,
) -> Result<(), String> {
    if !is_safe_path_component(&session_group) {
        return Err("invalid session group".into());
    }
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        if let Some(subspace) = app.subspaces.iter_mut().find(|s| s.id == subspace_id) {
            subspace.session_group = session_group;
        }
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
    Ok(())
}

#[tauri::command]
pub fn set_app_icon_style(
    app_handle: AppHandle,
    store: State<Store>,
    app_id: String,
    fit: String,
    rounded: bool,
    padding_percent: u8,
) {
    let mut config = store.config.lock().unwrap();
    let icon_data = config.apps.iter_mut().find(|a| a.id == app_id).map(|app| {
        app.icon_style.fit = fit;
        app.icon_style.rounded = rounded;
        app.icon_style.padding_percent = padding_percent;
        (app.icon.clone(), app.icon_background_color.clone(), app.name.clone(), app.data_slug.clone())
    });
    drop(config);
    store.save();

    let (icon_path, background, app_name, data_slug) = icon_data.unwrap_or_default();
    apply_window_icon(&app_handle, &app_id, icon_path.as_deref(), rounded, background.as_deref(), padding_percent);
    if let Some(tray) = app_handle.tray_by_id(&crate::layout::tray_id_for_app(&app_id)) {
        if let Some(icon) = resolve_window_icon(icon_path.as_deref(), rounded, background.as_deref(), padding_percent) {
            let _ = tray.set_icon(Some(icon));
        }
    }
    crate::shortcuts::create_shortcut(
        &app_id,
        &data_slug,
        &app_name,
        icon_path.as_deref(),
        background.as_deref(),
        padding_percent,
        rounded,
    );
    notify_sidebar(&app_handle, &app_id);
}

#[tauri::command]
pub fn set_subspace_icon_background(
    app_handle: AppHandle,
    store: State<Store>,
    app_id: String,
    subspace_id: String,
    background_color: Option<String>,
) {
    let mut config = store.config.lock().unwrap();
    if let Some(s) = config
        .apps
        .iter_mut()
        .find(|a| a.id == app_id)
        .and_then(|a| a.subspaces.iter_mut().find(|s| s.id == subspace_id))
    {
        s.icon_background_color = background_color;
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
}

#[tauri::command]
pub fn set_app_icon_background(app_handle: AppHandle, store: State<Store>, app_id: String, background_color: Option<String>) {
    let mut config = store.config.lock().unwrap();
    let icon_data = config.apps.iter_mut().find(|a| a.id == app_id).map(|app| {
        app.icon_background_color = background_color;
        (
            app.icon.clone(),
            app.icon_style.rounded,
            app.icon_style.padding_percent,
            app.icon_background_color.clone(),
            app.name.clone(),
            app.data_slug.clone(),
        )
    });
    drop(config);
    store.save();

    if let Some((icon_path, rounded, padding_percent, background, app_name, data_slug)) = icon_data {
        apply_window_icon(&app_handle, &app_id, icon_path.as_deref(), rounded, background.as_deref(), padding_percent);
        if let Some(tray) = app_handle.tray_by_id(&crate::layout::tray_id_for_app(&app_id)) {
            if let Some(icon) = resolve_window_icon(icon_path.as_deref(), rounded, background.as_deref(), padding_percent) {
                let _ = tray.set_icon(Some(icon));
            }
        }
        crate::shortcuts::create_shortcut(
            &app_id,
            &data_slug,
            &app_name,
            icon_path.as_deref(),
            background.as_deref(),
            padding_percent,
            rounded,
        );
    }
    notify_sidebar(&app_handle, &app_id);
}

#[tauri::command]
pub fn rename_app(app_handle: AppHandle, store: State<Store>, app_id: String, name: String) {
    let mut config = store.config.lock().unwrap();
    let old = config.apps.iter_mut().find(|a| a.id == app_id).map(|app| {
        let old_name = app.name.clone();
        app.name = name.clone();
        (
            old_name,
            app.icon.clone(),
            app.data_slug.clone(),
            app.icon_background_color.clone(),
            app.icon_style.padding_percent,
            app.icon_style.rounded,
        )
    });
    drop(config);
    store.save();
    if let Some((old_name, icon, data_slug, background, padding_percent, rounded)) = old {
        crate::shortcuts::rename_shortcut(
            &app_id,
            &data_slug,
            &old_name,
            &name,
            icon.as_deref(),
            background.as_deref(),
            padding_percent,
            rounded,
        );
    }
    notify_sidebar(&app_handle, &app_id);
}

#[tauri::command]
pub fn set_app_url(app_handle: AppHandle, store: State<Store>, app_id: String, url: String) {
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        app.url = url;
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
}

/// Closes every subspace's toolbar/tabs and deletes every session data
/// directory used by this app (deduplicated, since subspaces can share
/// a session_group).
#[tauri::command]
pub fn reset_app_sessions(app_handle: AppHandle, store: State<Store>, registry: State<TabRegistry>, app_id: String) -> Result<(), String> {
    let (data_slug, session_groups, subspace_ids): (String, std::collections::HashSet<String>, Vec<String>) = {
        let config = store.config.lock().unwrap();
        let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
        (
            app.data_slug.clone(),
            app.subspaces.iter().map(|s| s.session_group.clone()).collect(),
            app.subspaces.iter().map(|s| s.id.clone()).collect(),
        )
    };

    for subspace_id in &subspace_ids {
        close_subspace_views(&app_handle, &registry, &app_id, subspace_id);
    }

    for group in session_groups {
        let dir = safe_session_data_dir(&data_slug, &group)?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Window/webview creation deadlocks on Windows if called from a synchronous
/// command or from a webview event callback running on the main thread (it
/// tries to run itself on the main thread and blocks waiting on that same
/// thread). Marking these commands `async` moves them to a worker thread so
/// the internal main-thread dispatch can actually complete, and any webview
/// creation triggered from an event callback (e.g. on_new_window) is spawned
/// on a plain OS thread for the same reason. See:
/// https://github.com/tauri-apps/wry/issues/583
#[tauri::command]
pub async fn open_app_settings(app_handle: AppHandle, store: State<'_, Store>, app_id: String) -> Result<(), String> {
    let label = app_settings_window_label(&app_id);

    if let Some(window) = app_handle.get_window(&label) {
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let app_name = {
        let config = store.config.lock().unwrap();
        config
            .apps
            .iter()
            .find(|a| a.id == app_id)
            .map(|a| a.name.clone())
            .ok_or("app not found")?
    };

    let url = WebviewUrl::App(format!("index.html?settingsForApp={app_id}").into());

    tauri::WebviewWindowBuilder::new(&app_handle, &label, url)
        .title(format!("Impostazioni - {app_name}"))
        .inner_size(760.0, 520.0)
        .min_inner_size(560.0, 380.0)
        .background_color(APP_BG)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Shows a re-locked app's window with a full-window PIN overlay covering
/// its sidebar/toolbar/tab content, instead of bouncing through the
/// dashboard or a separate popup window. `unlock_app_window` reverses this
/// once the correct PIN comes back.
fn show_lock_overlay(app_handle: &AppHandle, app_id: &str) -> Result<(), String> {
    let window = app_handle
        .get_window(&app_window_label(app_id))
        .ok_or("app window not open")?;

    if let Some(sidebar) = app_handle.get_webview(&sidebar_label(app_id)) {
        let _ = sidebar.hide();
    }
    let toolbar_pfx = toolbar_prefix(app_id);
    let tab_pfx = tab_prefix_for_app(app_id);
    for (label, webview) in app_handle.webviews() {
        if label.starts_with(&toolbar_pfx) || label.starts_with(&tab_pfx) {
            let _ = webview.hide();
        }
    }

    let logical = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);
    let overlay_label = lock_overlay_label(app_id);

    if let Some(overlay) = app_handle.get_webview(&overlay_label) {
        overlay.show().map_err(|e| e.to_string())?;
        overlay
            .set_position(LogicalPosition::new(0.0, 0.0))
            .map_err(|e| e.to_string())?;
        overlay
            .set_size(LogicalSize::new(logical.width, logical.height))
            .map_err(|e| e.to_string())?;
    } else {
        let overlay_url = WebviewUrl::App(format!("index.html?unlockApp={app_id}").into());
        window
            .add_child(
                WebviewBuilder::new(&overlay_label, overlay_url).background_color(APP_BG),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(logical.width, logical.height),
            )
            .map_err(|e| e.to_string())?;
    }

    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Verifies the PIN for an app whose window is showing the lock overlay
/// (see `show_lock_overlay`) and, on success, hides the overlay and
/// restores its sidebar/toolbar/tab content.
#[tauri::command]
pub fn unlock_app_window(
    app_handle: AppHandle,
    store: State<Store>,
    registry: State<TabRegistry>,
    active: State<ActiveSubspaces>,
    widths: State<SidebarWidths>,
    app_id: String,
    pin: String,
) -> Result<(), String> {
    let subspace_id = {
        let config = store.config.lock().unwrap();
        let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
        if let Some(expected) = &app.pin_hash {
            if !verify_pin(expected, &pin) {
                return Err("invalid pin".to_string());
            }
        }
        active
            .0
            .lock()
            .unwrap()
            .get(&app_id)
            .cloned()
            .or_else(|| app.subspaces.first().map(|s| s.id.clone()))
    };

    let locked = app_handle.state::<LockedApps>();
    locked.0.lock().unwrap().remove(&app_id);

    if let Some(overlay) = app_handle.get_webview(&lock_overlay_label(&app_id)) {
        let _ = overlay.hide();
    }
    if let Some(sidebar) = app_handle.get_webview(&sidebar_label(&app_id)) {
        let _ = sidebar.show();
    }

    if let Some(subspace_id) = subspace_id {
        let sidebar_width = widths.get(&app_id);
        show_subspace_tabs(&app_handle, &store, &registry, sidebar_width, &app_id, &subspace_id)?;
    }

    let background = app_handle.state::<BackgroundApps>();
    background.0.lock().unwrap().remove(&app_id);
    let _ = app_handle.remove_tray_by_id(&crate::layout::tray_id_for_app(&app_id));

    Ok(())
}

/// Cancels an in-progress unlock: hides the app back into the tray instead
/// of leaving its window sitting there locked and visible.
#[tauri::command]
pub fn cancel_app_unlock(app_handle: AppHandle, app_id: String) -> Result<(), String> {
    let window = app_handle
        .get_window(&app_window_label(&app_id))
        .ok_or("app window not open")?;
    window.hide().map_err(|e| e.to_string())?;
    let background = app_handle.state::<BackgroundApps>();
    background.0.lock().unwrap().insert(app_id.clone());
    spawn_background_tray(&app_handle, &app_id);
    Ok(())
}

#[tauri::command]
pub async fn open_global_settings(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_window(GLOBAL_SETTINGS_LABEL) {
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let url = WebviewUrl::App("index.html?globalSettings=1".into());

    tauri::WebviewWindowBuilder::new(&app_handle, GLOBAL_SETTINGS_LABEL, url)
        .title("Impostazioni globali")
        .inner_size(480.0, 420.0)
        .min_inner_size(420.0, 360.0)
        .background_color(APP_BG)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Opens (or focuses) the main dashboard window, e.g. from a webapp
/// window's sidebar shortcut between "+" and settings.
#[tauri::command]
pub async fn open_dashboard(app_handle: AppHandle) {
    crate::show_dashboard(&app_handle);
}

#[tauri::command]
pub fn rename_subspace(app_handle: AppHandle, store: State<Store>, app_id: String, subspace_id: String, name: String) {
    let mut config = store.config.lock().unwrap();
    if let Some(s) = config
        .apps
        .iter_mut()
        .find(|a| a.id == app_id)
        .and_then(|a| a.subspaces.iter_mut().find(|s| s.id == subspace_id))
    {
        s.name = name;
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
}

#[tauri::command]
pub fn reorder_subspaces(app_handle: AppHandle, store: State<Store>, app_id: String, subspace_ids: Vec<String>) {
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        // Any subspace id missing from the incoming order (a stale/partial
        // list) sorts to the end rather than erroring — reordering is
        // best-effort, not validated against the current subspace set.
        app.subspaces.sort_by_key(|s| {
            subspace_ids
                .iter()
                .position(|id| *id == s.id)
                .unwrap_or(usize::MAX)
        });
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
}

#[tauri::command]
pub fn set_subspace_start_url(
    app_handle: AppHandle,
    store: State<Store>,
    app_id: String,
    subspace_id: String,
    start_url: Option<String>,
) {
    let mut config = store.config.lock().unwrap();
    if let Some(s) = config
        .apps
        .iter_mut()
        .find(|a| a.id == app_id)
        .and_then(|a| a.subspaces.iter_mut().find(|s| s.id == subspace_id))
    {
        s.start_url = start_url.filter(|u| !u.trim().is_empty());
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
}

/// Hashes a PIN with Argon2id and a fresh random salt (stored inline in the
/// returned PHC string, so the config file never holds the PIN itself and
/// every app gets an independent salt even for the same PIN).
fn hash_pin(pin: &str) -> Result<String, String> {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// Verifies a PIN against a PHC hash produced by `hash_pin`.
fn verify_pin(hash: &str, pin: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    use argon2::Argon2;
    let Ok(parsed) = PasswordHash::new(hash) else { return false };
    Argon2::default().verify_password(pin.as_bytes(), &parsed).is_ok()
}

#[tauri::command]
pub fn set_app_pin(store: State<Store>, app_id: String, pin: Option<String>) -> Result<(), String> {
    let pin_hash = pin
        .filter(|p| !p.trim().is_empty())
        .map(|p| hash_pin(&p))
        .transpose()?;

    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        app.pin_hash = pin_hash;
        if app.pin_hash.is_none() {
            app.pin_lock_on_background = false;
        }
    }
    drop(config);
    store.save();
    Ok(())
}

/// Configures whether this app re-locks itself when backgrounded to the
/// tray, and after how long. `lock_on_background` is ignored (forced off)
/// when the app has no PIN set.
#[tauri::command]
pub fn set_app_pin_lock(
    store: State<Store>,
    app_id: String,
    lock_on_background: bool,
    delay_secs: u64,
) {
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        app.pin_lock_on_background = lock_on_background && app.pin_hash.is_some();
        app.pin_lock_delay_secs = delay_secs;
    }
    drop(config);
    store.save();
}

#[tauri::command]
pub async fn open_app(
    app_handle: AppHandle,
    store: State<'_, Store>,
    registry: State<'_, TabRegistry>,
    widths: State<'_, SidebarWidths>,
    app_id: String,
    pin: Option<String>,
) -> Result<(), String> {
    let label = app_window_label(&app_id);

    // Window already exists (e.g. backgrounded in the tray): normally it was
    // already unlocked for this session, so just restore it. If it re-locked
    // itself in the tray though, the PIN is required again before restoring.
    if let Some(window) = app_handle.get_window(&label) {
        let locked = app_handle.state::<LockedApps>();
        if locked.0.lock().unwrap().contains(&app_id) {
            let config = store.config.lock().unwrap();
            let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
            if let Some(expected) = &app.pin_hash {
                let provided = pin.clone().unwrap_or_default();
                if !verify_pin(expected, &provided) {
                    return Err("invalid pin".to_string());
                }
            }
            drop(config);
            locked.0.lock().unwrap().remove(&app_id);

            // In case a lock overlay was raised (e.g. via the tray) before
            // this call, undo it: hide it and restore the real content.
            if let Some(overlay) = app_handle.get_webview(&lock_overlay_label(&app_id)) {
                let _ = overlay.hide();
            }
            if let Some(sidebar) = app_handle.get_webview(&sidebar_label(&app_id)) {
                let _ = sidebar.show();
            }
            let subspace_id = {
                let active = app_handle.state::<ActiveSubspaces>();
                let active_id = active.0.lock().unwrap().get(&app_id).cloned();
                match active_id {
                    Some(id) => Some(id),
                    None => {
                        let config = store.config.lock().unwrap();
                        config.apps.iter().find(|a| a.id == app_id).and_then(|a| a.subspaces.first().map(|s| s.id.clone()))
                    }
                }
            };
            if let Some(subspace_id) = subspace_id {
                let sidebar_width = widths.get(&app_id);
                show_subspace_tabs(&app_handle, &store, &registry, sidebar_width, &app_id, &subspace_id)?;
            }
        }

        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        let background = app_handle.state::<BackgroundApps>();
        background.0.lock().unwrap().remove(&app_id);
        let _ = app_handle.remove_tray_by_id(&crate::layout::tray_id_for_app(&app_id));
        return Ok(());
    }

    // Shortcut launches carry no PIN, so a protected app can't be verified
    // up front: rather than erroring out (which used to bounce the user to
    // the dashboard instead of the app they clicked), the window is opened
    // as usual and immediately covered with the same lock overlay used for
    // re-locked backgrounded apps, prompting for the PIN in place.
    let needs_pin = {
        let config = store.config.lock().unwrap();
        let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
        match &app.pin_hash {
            Some(expected) => !verify_pin(expected, &pin.unwrap_or_default()),
            None => false,
        }
    };

    let (app_name, app_icon, icon_rounded, icon_padding, icon_background, data_slug, first_subspace, eager_load_subspaces, other_subspace_ids) = {
        let config = store.config.lock().unwrap();
        let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
        let first_subspace = app.subspaces.first().map(|s| s.id.clone());
        let other_subspace_ids = app
            .subspaces
            .iter()
            .map(|s| s.id.clone())
            .filter(|id| Some(id) != first_subspace.as_ref())
            .collect::<Vec<_>>();
        (
            app.name.clone(),
            app.icon.clone(),
            app.icon_style.rounded,
            app.icon_style.padding_percent,
            app.icon_background_color.clone(),
            app.data_slug.clone(),
            first_subspace,
            app.eager_load_subspaces,
            other_subspace_ids,
        )
    };

    // Built hidden and only shown once the window is fully ready below (real
    // content, or the lock overlay covering it if a PIN is required) — a
    // visible-by-default window would flash the unprotected content for a
    // frame while the subspace webviews and lock overlay are still loading.
    let window = WindowBuilder::new(&app_handle, &label)
        .title(&app_name)
        .inner_size(1100.0, 750.0)
        .background_color(APP_BG)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;

    if let Some(icon) = resolve_window_icon(app_icon.as_deref(), icon_rounded, icon_background.as_deref(), icon_padding) {
        let _ = window.set_icon(icon);
    }
    #[cfg(windows)]
    crate::platform::windows::winid::set_window_app_id(
        window.hwnd(),
        |f| window.run_on_main_thread(f),
        &crate::platform::windows::winid::aumid_for_slug("Silos.WebApp", &data_slug),
    );

    // Regenerated on every launch, not just on explicit icon/style edits:
    // the Start Menu shortcut shares this window's AUMID, and Windows can
    // prefer that registered shortcut icon over the live window icon for
    // the taskbar button — so it has to already match icon_style, not just
    // eventually catch up next time some settings field is touched.
    crate::shortcuts::create_shortcut(
        &app_id,
        &data_slug,
        &app_name,
        app_icon.as_deref(),
        icon_background.as_deref(),
        icon_padding,
        icon_rounded,
    );

    let logical_size = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(window.scale_factor().map_err(|e| e.to_string())?);

    let sidebar_width = widths.get(&app_id);
    let sidebar_url = WebviewUrl::App(format!("index.html?appId={app_id}").into());
    window
        .add_child(
            WebviewBuilder::new(sidebar_label(&app_id), sidebar_url).background_color(APP_BG),
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(sidebar_width, logical_size.height),
        )
        .map_err(|e| e.to_string())?;

    if let Some(subspace_id) = &first_subspace {
        show_subspace_tabs(&app_handle, &store, &registry, sidebar_width, &app_id, subspace_id)?;

        // Creates+loads every other subspace's tab too (each `show_subspace_tabs`
        // call below hides whatever was shown by the previous one), so a
        // subspace the user hasn't clicked into yet — a second WhatsApp
        // account, say — is still running and can still surface
        // notifications. The final call restores the originally active one
        // as the only visible subspace; best-effort (`let _`) per subspace
        // so one bad subspace URL doesn't block the rest from loading.
        if eager_load_subspaces {
            for other_id in &other_subspace_ids {
                let _ = show_subspace_tabs(&app_handle, &store, &registry, sidebar_width, &app_id, other_id);
            }
            show_subspace_tabs(&app_handle, &store, &registry, sidebar_width, &app_id, subspace_id)?;
        }
    }

    if needs_pin {
        let locked = app_handle.state::<LockedApps>();
        locked.0.lock().unwrap().insert(app_id.clone());
        show_lock_overlay(&app_handle, &app_id)?;
    }

    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;

    let event_handle = app_handle.clone();
    let event_app_id = app_id.clone();
    let event_window = window.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) => resize_children(&event_handle, &event_app_id),
        tauri::WindowEvent::CloseRequested { api, .. } => {
            if app_runs_in_background(&event_handle, &event_app_id) {
                api.prevent_close();
                let _ = event_window.hide();
                background_app(&event_handle, &event_app_id);
            }
        }
        tauri::WindowEvent::Destroyed => {
            remember_last_tabs_and_clear(&event_handle, &event_app_id);
            let locked = event_handle.state::<LockedApps>();
            locked.0.lock().unwrap().remove(&event_app_id);
        }
        _ => {}
    });

    Ok(())
}

fn app_runs_in_background(app_handle: &AppHandle, app_id: &str) -> bool {
    let store = app_handle.state::<Store>();
    let config = store.config.lock().unwrap();
    config.apps.iter().any(|a| a.id == app_id && a.run_in_background)
}

/// Hides an app's window into the tray instead of letting it close: marks it
/// as backgrounded and gives it its own tray icon so it can be managed
/// independently of any other backgrounded app.
fn background_app(app_handle: &AppHandle, app_id: &str) {
    let background = app_handle.state::<BackgroundApps>();
    background.0.lock().unwrap().insert(app_id.to_string());
    spawn_background_tray(app_handle, app_id);
    schedule_pin_lock(app_handle, app_id);
}

/// If this app has "lock on background" enabled, re-locks it (immediately,
/// or after its configured delay if it's still backgrounded then) so
/// bringing it back requires the PIN again.
fn schedule_pin_lock(app_handle: &AppHandle, app_id: &str) {
    let (should_lock, delay_secs) = {
        let store = app_handle.state::<Store>();
        let config = store.config.lock().unwrap();
        match config.apps.iter().find(|a| a.id == app_id) {
            Some(a) => (a.pin_hash.is_some() && a.pin_lock_on_background, a.pin_lock_delay_secs),
            None => (false, 0),
        }
    };
    if !should_lock {
        return;
    }
    if delay_secs == 0 {
        lock_app(app_handle, app_id);
        return;
    }

    let handle = app_handle.clone();
    let id = app_id.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(delay_secs));
        // Only lock if it's still sitting in the tray (user might have
        // reopened it before the delay elapsed).
        let background = handle.state::<BackgroundApps>();
        if background.0.lock().unwrap().contains(&id) {
            lock_app(&handle, &id);
        }
    });
}

fn lock_app(app_handle: &AppHandle, app_id: &str) {
    let locked = app_handle.state::<LockedApps>();
    locked.0.lock().unwrap().insert(app_id.to_string());
}

/// Creates the tray icon for a single backgrounded app: its own icon,
/// tooltip with its name, left-click to restore it, right-click for a
/// Show/Close menu. No-op if that app already has one.
fn spawn_background_tray(app_handle: &AppHandle, app_id: &str) {
    let tray_id = crate::layout::tray_id_for_app(app_id);
    if app_handle.tray_by_id(&tray_id).is_some() {
        return;
    }

    let (app_name, icon_path, icon_rounded, icon_padding, icon_background) = {
        let store = app_handle.state::<Store>();
        let config = store.config.lock().unwrap();
        match config.apps.iter().find(|a| a.id == app_id) {
            Some(a) => (
                a.name.clone(),
                a.icon.clone(),
                a.icon_style.rounded,
                a.icon_style.padding_percent,
                a.icon_background_color.clone(),
            ),
            None => return,
        }
    };

    let Ok(menu) = MenuBuilder::new(app_handle)
        .text(format!("tray-show|{app_id}"), "Mostra")
        .text(format!("tray-close|{app_id}"), "Chiudi")
        .build()
    else {
        return;
    };

    let mut builder = tauri::tray::TrayIconBuilder::with_id(tray_id)
        .tooltip(&app_name)
        .menu(&menu)
        .show_menu_on_left_click(false);

    if let Some(icon) = resolve_window_icon(icon_path.as_deref(), icon_rounded, icon_background.as_deref(), icon_padding) {
        builder = builder.icon(icon);
    }

    let click_handle = app_handle.clone();
    let click_app_id = app_id.to_string();
    builder = builder.on_tray_icon_event(move |_tray, event| {
        if let tauri::tray::TrayIconEvent::Click {
            button: tauri::tray::MouseButton::Left,
            button_state: tauri::tray::MouseButtonState::Up,
            ..
        } = event
        {
            let _ = show_background_app(click_handle.clone(), click_app_id.clone());
        }
    });

    let _ = builder.build(app_handle);
}

/// Brings a backgrounded app's window back (tray icon click or "Mostra").
/// If it re-locked itself while backgrounded, this shows the window with a
/// PIN overlay over its content instead of restoring it outright; the
/// overlay itself calls `unlock_app_window` once a correct PIN comes in.
#[tauri::command]
pub fn show_background_app(app_handle: AppHandle, app_id: String) -> Result<(), String> {
    let locked = app_handle.state::<LockedApps>();
    if locked.0.lock().unwrap().contains(&app_id) {
        show_lock_overlay(&app_handle, &app_id)?;
        return Ok(());
    }

    let window = app_handle
        .get_window(&app_window_label(&app_id))
        .ok_or("app window not open")?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;

    let background = app_handle.state::<BackgroundApps>();
    background.0.lock().unwrap().remove(&app_id);
    let _ = app_handle.remove_tray_by_id(&crate::layout::tray_id_for_app(&app_id));
    Ok(())
}

/// Fully closes a backgrounded app's window ("Chiudi" in its tray menu),
/// bypassing the run_in_background hide-instead-of-close behavior — destroy()
/// skips CloseRequested entirely so it can't just get hidden again.
pub fn close_background_app(app_handle: &AppHandle, app_id: &str) {
    if let Some(window) = app_handle.get_window(&app_window_label(app_id)) {
        let _ = window.destroy();
    }
    let background = app_handle.state::<BackgroundApps>();
    background.0.lock().unwrap().remove(app_id);
    let locked = app_handle.state::<LockedApps>();
    locked.0.lock().unwrap().remove(app_id);
    let _ = app_handle.remove_tray_by_id(&crate::layout::tray_id_for_app(app_id));
}

/// Same as `close_background_app`, exposed to the frontend so a visible
/// (not just backgrounded) app can quit outright from its own sidebar,
/// instead of forcing the user through the tray icon's context menu.
#[tauri::command]
pub fn quit_app(app_handle: AppHandle, app_id: String) {
    close_background_app(&app_handle, &app_id);
}

#[tauri::command]
pub fn set_app_run_in_background(app_handle: AppHandle, store: State<Store>, app_id: String, enabled: bool) {
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        app.run_in_background = enabled;
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
}

#[tauri::command]
pub fn set_app_eager_load_subspaces(app_handle: AppHandle, store: State<Store>, app_id: String, enabled: bool) {
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        app.eager_load_subspaces = enabled;
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
}

/// Takes effect for tabs opened after this is set — doesn't retroactively
/// patch webviews already running, since the bypass is wired once at webview
/// creation (see `open_tab_internal`).
#[tauri::command]
pub fn set_app_download_mode(store: State<Store>, app_id: String, mode: String) {
    if mode != "downloads" && mode != "ask" {
        return;
    }
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        app.download_mode = mode;
    }
    drop(config);
    store.save();
}

#[tauri::command]
pub fn set_app_ignore_certificate_errors(store: State<Store>, app_id: String, enabled: bool) {
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        app.ignore_certificate_errors = enabled;
    }
    drop(config);
    store.save();
}

#[tauri::command]
pub fn set_app_hibernate_delay_secs(app_handle: AppHandle, store: State<Store>, app_id: String, delay_secs: u64) {
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        app.hibernate_delay_secs = delay_secs;
    }
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
}

/// Closing an app's window destroys every subspace's tab webviews (they're
/// children of that window), so on reopen there'd be nothing to show and a
/// fresh blank tab would be created at the app/subspace's default URL. To
/// make reopening land back where the user left off, each subspace's last
/// active tab URL is saved as its start_url, and the now-dangling tab
/// entries are dropped from the registry so the toolbar doesn't show tabs
/// whose webviews no longer exist.
fn remember_last_tabs_and_clear(app_handle: &AppHandle, app_id: &str) {
    let store = app_handle.state::<Store>();
    let registry = app_handle.state::<TabRegistry>();
    let prefix = format!("{app_id}::");

    let mut reg = registry.0.lock().unwrap();
    let keys: Vec<String> = reg.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();

    if !keys.is_empty() {
        let mut config = store.config.lock().unwrap();
        if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
            for key in &keys {
                let Some(subspace_id) = key.strip_prefix(&prefix) else { continue };
                let Some(active_url) = reg
                    .get(key)
                    .and_then(|t| t.tabs.iter().find(|tab| tab.id == t.active))
                    .map(|tab| tab.url.clone())
                else {
                    continue;
                };
                if let Some(subspace) = app.subspaces.iter_mut().find(|s| s.id == subspace_id) {
                    subspace.start_url = Some(active_url);
                }
            }
        }
        drop(config);
        store.save();
    }

    for key in &keys {
        reg.remove(key);
    }
}

fn resize_children(app_handle: &AppHandle, app_id: &str) {
    let Some(window) = app_handle.get_window(&app_window_label(app_id)) else {
        return;
    };
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    let logical_size = size.to_logical::<f64>(scale);
    let sidebar_width = app_handle.state::<SidebarWidths>().get(app_id);
    let content_width = (logical_size.width - sidebar_width).max(0.0);
    let tab_height = (logical_size.height - TOOLBAR_HEIGHT).max(0.0);

    if let Some(sidebar) = app_handle.get_webview(&sidebar_label(app_id)) {
        let _ = sidebar.set_size(LogicalSize::new(sidebar_width, logical_size.height));
    }

    let toolbar_prefix = toolbar_prefix(app_id);
    for (label, webview) in app_handle.webviews() {
        if label.starts_with(&toolbar_prefix) {
            let _ = webview.set_position(LogicalPosition::new(sidebar_width, 0.0));
            let _ = webview.set_size(LogicalSize::new(content_width, TOOLBAR_HEIGHT));
        }
    }

    let tab_prefix = tab_prefix_for_app(app_id);
    for (label, webview) in app_handle.webviews() {
        if label.starts_with(&tab_prefix) {
            let _ = webview.set_position(LogicalPosition::new(sidebar_width, TOOLBAR_HEIGHT));
            let _ = webview.set_size(LogicalSize::new(content_width, tab_height));
        }
    }

    if let Some(overlay) = app_handle.get_webview(&lock_overlay_label(app_id)) {
        let _ = overlay.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = overlay.set_size(LogicalSize::new(logical_size.width, logical_size.height));
    }
}

#[tauri::command]
pub async fn select_subspace(
    app_handle: AppHandle,
    store: State<'_, Store>,
    registry: State<'_, TabRegistry>,
    active: State<'_, ActiveSubspaces>,
    widths: State<'_, SidebarWidths>,
    app_id: String,
    subspace_id: String,
) -> Result<(), String> {
    let sidebar_width = widths.get(&app_id);

    let eager_load = {
        let config = store.config.lock().unwrap();
        config.apps.iter().find(|a| a.id == app_id).map(|a| a.eager_load_subspaces).unwrap_or(true)
    };
    if !eager_load {
        let previous_id = active.0.lock().unwrap().get(&app_id).cloned();
        if let Some(previous_id) = previous_id {
            if previous_id != subspace_id {
                let delay_secs = {
                    let config = store.config.lock().unwrap();
                    config.apps.iter().find(|a| a.id == app_id).map(|a| a.hibernate_delay_secs).unwrap_or(300)
                };
                schedule_subspace_hibernate(&app_handle, &app_id, &previous_id, delay_secs);
            }
        }
    }

    show_subspace_tabs(&app_handle, &store, &registry, sidebar_width, &app_id, &subspace_id)?;
    active.0.lock().unwrap().insert(app_id, subspace_id);
    Ok(())
}

/// Closes a subspace's webview(s) after `delay_secs`, unless the user has
/// switched back to it before then (checked right before acting — an
/// implicit cancel, same pattern as `schedule_pin_lock`'s still-backgrounded
/// check). Only ever called when `eager_load_subspaces` is off; the app
/// could have several of these pending at once for different subspaces (or
/// even the same one, from repeated quick switches) — harmless, each just
/// re-checks and no-ops if there's nothing left to close.
fn schedule_subspace_hibernate(app_handle: &AppHandle, app_id: &str, subspace_id: &str, delay_secs: u64) {
    let handle = app_handle.clone();
    let app_id = app_id.to_string();
    let subspace_id = subspace_id.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(delay_secs));
        let active = handle.state::<ActiveSubspaces>();
        let still_inactive = active.0.lock().unwrap().get(&app_id).map(|a| a != &subspace_id).unwrap_or(true);
        if still_inactive {
            remember_subspace_start_url(&handle, &app_id, &subspace_id);
            let registry = handle.state::<TabRegistry>();
            close_subspace_views(&handle, &registry, &app_id, &subspace_id);
        }
    });
}

/// Saves a subspace's currently active tab URL as its `start_url`, so
/// closing its webview(s) (hibernation, see `schedule_subspace_hibernate`)
/// can reload the same page next time it's selected instead of resetting to
/// the app/subspace's original default. Same one-tab-remembered tradeoff
/// `remember_last_tabs_and_clear` already makes for a full app close: extra
/// tabs beyond the active one aren't preserved.
fn remember_subspace_start_url(app_handle: &AppHandle, app_id: &str, subspace_id: &str) {
    let registry = app_handle.state::<TabRegistry>();
    let key = subspace_key(app_id, subspace_id);
    let active_url = {
        let reg = registry.0.lock().unwrap();
        reg.get(&key).and_then(|t| t.tabs.iter().find(|tab| tab.id == t.active)).map(|tab| tab.url.clone())
    };
    let Some(url) = active_url else { return };

    let store = app_handle.state::<Store>();
    let mut config = store.config.lock().unwrap();
    if let Some(app) = config.apps.iter_mut().find(|a| a.id == app_id) {
        if let Some(subspace) = app.subspaces.iter_mut().find(|s| s.id == subspace_id) {
            subspace.start_url = Some(url);
        }
    }
    drop(config);
    store.save();
}

/// Opens a small borderless popup window for the "add subspace" input.
/// Same reasoning as the native context menu: a floating input inside the
/// sidebar webview would be clipped by that webview's own (narrow) bounds,
/// so this uses a real separate OS window positioned near the sidebar
/// instead, which isn't constrained by any webview's surface size.
#[tauri::command]
pub async fn open_add_subspace_popup(app_handle: AppHandle, widths: State<'_, SidebarWidths>, app_id: String) -> Result<(), String> {
    let label = add_popup_label(&app_id);

    if let Some(window) = app_handle.get_window(&label) {
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let app_window = app_handle
        .get_window(&app_window_label(&app_id))
        .ok_or("app window not open")?;

    let scale = app_window.scale_factor().map_err(|e| e.to_string())?;
    let origin = app_window
        .outer_position()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    let window_size = app_window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    let sidebar_width = widths.get(&app_id);

    // Big enough to fit the icon picker's two-column source buttons and the
    // size preview without either scrollbar; a real titlebar (decorations,
    // resizable) so it behaves like a normal window instead of a cramped
    // borderless popup.
    let popup_width = 640.0;
    let popup_height = 540.0;

    let popup_x = origin.x + sidebar_width + 8.0;
    let popup_y = (origin.y + window_size.height - popup_height - 62.0).max(origin.y + 8.0);

    let url = WebviewUrl::App(format!("index.html?addPopupFor={app_id}").into());

    tauri::WebviewWindowBuilder::new(&app_handle, &label, url)
        .title("Nuovo sottospazio")
        .inner_size(popup_width, popup_height)
        .min_inner_size(560.0, 520.0)
        .position(popup_x, popup_y)
        .decorations(true)
        .resizable(true)
        .focused(true)
        .background_color(APP_BG)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Small borderless popup asking to confirm a full quit (bypassing
/// run_in_background), anchored near the sidebar's power button — same
/// positioning approach as `open_add_subspace_popup`, just tiny since it's
/// only ever a yes/no prompt.
#[tauri::command]
pub async fn open_quit_confirm_popup(app_handle: AppHandle, app_id: String) -> Result<(), String> {
    let label = quit_confirm_label(&app_id);

    if let Some(window) = app_handle.get_window(&label) {
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let app_window = app_handle
        .get_window(&app_window_label(&app_id))
        .ok_or("app window not open")?;

    let scale = app_window.scale_factor().map_err(|e| e.to_string())?;
    let origin = app_window
        .outer_position()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    let window_size = app_window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);

    let popup_width = 300.0;
    let popup_height = 150.0;

    let popup_x = origin.x + 8.0;
    let popup_y = (origin.y + window_size.height - popup_height - 62.0).max(origin.y + 8.0);

    let url = WebviewUrl::App(format!("index.html?quitConfirmFor={app_id}").into());

    tauri::WebviewWindowBuilder::new(&app_handle, &label, url)
        .title("Chiudere l'app?")
        .inner_size(popup_width, popup_height)
        .position(popup_x, popup_y)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(true)
        .background_color(APP_BG)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn get_sidebar_expanded(widths: State<SidebarWidths>, app_id: String) -> bool {
    widths.get(&app_id) > SIDEBAR_WIDTH_COLLAPSED
}

#[tauri::command]
pub fn toggle_sidebar(app_handle: AppHandle, widths: State<SidebarWidths>, app_id: String, expanded: bool) {
    let width = if expanded { SIDEBAR_WIDTH_EXPANDED } else { SIDEBAR_WIDTH_COLLAPSED };
    widths.set(&app_id, width);
    resize_children(&app_handle, &app_id);
}

/// Ensures the given subspace's toolbar and (at least one) tab are visible,
/// hiding every other subspace's toolbar/tabs in the same app window.
fn show_subspace_tabs(
    app_handle: &AppHandle,
    store: &State<Store>,
    registry: &State<TabRegistry>,
    sidebar_width: f64,
    app_id: &str,
    subspace_id: &str,
) -> Result<(), String> {
    let window = app_handle
        .get_window(&app_window_label(app_id))
        .ok_or("app window not open")?;

    let target_toolbar = toolbar_label(app_id, subspace_id);
    let toolbar_pfx = toolbar_prefix(app_id);
    for (label, webview) in app_handle.webviews() {
        if label.starts_with(&toolbar_pfx) && label != target_toolbar {
            let _ = webview.hide();
        }
    }

    let target_tab_prefix = tab_prefix_for_subspace(app_id, subspace_id);
    let all_tabs_prefix = tab_prefix_for_app(app_id);
    for (label, webview) in app_handle.webviews() {
        if label.starts_with(&all_tabs_prefix) && !label.starts_with(&target_tab_prefix) {
            let _ = webview.hide();
        }
    }

    let (content_width, window_height) = content_geometry(&window, sidebar_width)?;

    if let Some(toolbar) = app_handle.get_webview(&target_toolbar) {
        toolbar.show().map_err(|e| e.to_string())?;
        toolbar
            .set_position(LogicalPosition::new(sidebar_width, 0.0))
            .map_err(|e| e.to_string())?;
        toolbar
            .set_size(LogicalSize::new(content_width, TOOLBAR_HEIGHT))
            .map_err(|e| e.to_string())?;
    } else {
        let toolbar_url = WebviewUrl::App(format!("index.html?toolbarApp={app_id}&toolbarSubspace={subspace_id}").into());
        window
            .add_child(
                WebviewBuilder::new(&target_toolbar, toolbar_url).background_color(APP_BG),
                LogicalPosition::new(sidebar_width, 0.0),
                LogicalSize::new(content_width, TOOLBAR_HEIGHT),
            )
            .map_err(|e| e.to_string())?;
    }

    let key = subspace_key(app_id, subspace_id);
    let active_tab_id = registry.0.lock().unwrap().get(&key).map(|t| t.active.clone());

    if let Some(tab_id) = active_tab_id {
        let label = tab_label(app_id, subspace_id, &tab_id);
        if let Some(webview) = app_handle.get_webview(&label) {
            webview.show().map_err(|e| e.to_string())?;
            webview
                .set_position(LogicalPosition::new(sidebar_width, TOOLBAR_HEIGHT))
                .map_err(|e| e.to_string())?;
            webview
                .set_size(LogicalSize::new(content_width, (window_height - TOOLBAR_HEIGHT).max(0.0)))
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    }

    open_tab_internal(app_handle, store, registry, sidebar_width, app_id, subspace_id, None)?;
    Ok(())
}

fn notify_downloads(app_handle: &AppHandle, app_id: &str) {
    let _ = app_handle.emit("downloads-changed", app_id);
}

/// Decides where a page-initiated download goes and keeps the app's download
/// list in sync. Returning false on `Requested` cancels the download.
fn handle_download_event(app_handle: &AppHandle, app_id: &str, event: tauri::webview::DownloadEvent) -> bool {
    use tauri::webview::DownloadEvent;
    use tauri_plugin_dialog::DialogExt;

    let registry = app_handle.state::<DownloadRegistry>();
    match event {
        DownloadEvent::Requested { url, destination } => {
            let ask = {
                let store = app_handle.state::<Store>();
                let config = store.config.lock().unwrap();
                config.apps.iter().any(|a| a.id == app_id && a.download_mode == "ask")
            };
            let file_name = destination
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "download".to_string());
            let downloads_dir = app_handle.path().download_dir().ok();

            if ask {
                let mut dialog = app_handle.dialog().file().set_file_name(&file_name);
                if let Some(dir) = &downloads_dir {
                    dialog = dialog.set_directory(dir);
                }
                // A cancelled dialog means no download at all.
                let Some(chosen) = dialog.blocking_save_file().and_then(|p| p.into_path().ok()) else {
                    return false;
                };
                *destination = chosen;
            } else if let Some(dir) = &downloads_dir {
                *destination = crate::downloads::unique_path(dir, &file_name);
            }

            registry.start(app_id, url.as_str(), destination);
            notify_downloads(app_handle, app_id);
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            registry.finish(app_id, url.as_str(), path.as_deref(), success);
            notify_downloads(app_handle, app_id);
            true
        }
        _ => true,
    }
}

#[tauri::command]
pub fn list_downloads(registry: State<DownloadRegistry>, app_id: String) -> Vec<crate::downloads::DownloadItem> {
    registry.list(&app_id)
}

#[tauri::command]
pub fn clear_downloads(app_handle: AppHandle, registry: State<DownloadRegistry>, app_id: String) {
    registry.clear(&app_id);
    notify_downloads(&app_handle, &app_id);
}

#[tauri::command]
pub fn open_download(app_handle: AppHandle, registry: State<DownloadRegistry>, app_id: String, id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let item = registry.find(&app_id, &id).ok_or("download not found")?;
    app_handle.opener().open_path(item.path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_download(app_handle: AppHandle, registry: State<DownloadRegistry>, app_id: String, id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let item = registry.find(&app_id, &id).ok_or("download not found")?;
    app_handle.opener().reveal_item_in_dir(item.path).map_err(|e| e.to_string())
}

/// Borderless popup listing the app's downloads, anchored bottom-right of the
/// app window (same approach as `open_quit_confirm_popup`).
#[tauri::command]
pub async fn open_downloads_popup(app_handle: AppHandle, app_id: String) -> Result<(), String> {
    let label = downloads_popup_label(&app_id);

    if let Some(window) = app_handle.get_window(&label) {
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let app_window = app_handle
        .get_window(&app_window_label(&app_id))
        .ok_or("app window not open")?;

    let scale = app_window.scale_factor().map_err(|e| e.to_string())?;
    let origin = app_window.outer_position().map_err(|e| e.to_string())?.to_logical::<f64>(scale);
    let window_size = app_window.inner_size().map_err(|e| e.to_string())?.to_logical::<f64>(scale);

    let popup_width = 360.0;
    let popup_height = 420.0;
    let popup_x = (origin.x + window_size.width - popup_width - 8.0).max(origin.x + 8.0);
    let popup_y = origin.y + TOOLBAR_HEIGHT + 4.0;

    let url = WebviewUrl::App(format!("index.html?downloadsPopupFor={app_id}").into());

    tauri::WebviewWindowBuilder::new(&app_handle, &label, url)
        .title("Download")
        .inner_size(popup_width, popup_height)
        .position(popup_x, popup_y)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(true)
        .background_color(APP_BG)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Creates a new tab (webview) for a subspace, wiring up navigation/title
/// sync and new-window interception, and makes it the active tab.
fn open_tab_internal(
    app_handle: &AppHandle,
    store: &State<Store>,
    registry: &State<TabRegistry>,
    sidebar_width: f64,
    app_id: &str,
    subspace_id: &str,
    url_override: Option<String>,
) -> Result<String, String> {
    let window = app_handle
        .get_window(&app_window_label(app_id))
        .ok_or("app window not open")?;

    let (url, data_slug, session_group, ignore_certificate_errors) = {
        let config = store.config.lock().unwrap();
        let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
        let subspace = app
            .subspaces
            .iter()
            .find(|s| s.id == subspace_id)
            .ok_or("subspace not found")?;
        let default_url = subspace.start_url.clone().unwrap_or_else(|| app.url.clone());
        (
            url_override.unwrap_or(default_url),
            app.data_slug.clone(),
            subspace.session_group.clone(),
            app.ignore_certificate_errors,
        )
    };

    let data_dir = safe_session_data_dir(&data_slug, &session_group)?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let (content_width, window_height) = content_geometry(&window, sidebar_width)?;
    let tab_height = (window_height - TOOLBAR_HEIGHT).max(0.0);

    let prefix = tab_prefix_for_subspace(app_id, subspace_id);
    for (label, webview) in app_handle.webviews() {
        if label.starts_with(&prefix) {
            let _ = webview.hide();
        }
    }

    let tab_id = uuid::Uuid::new_v4().to_string();
    let label = tab_label(app_id, subspace_id, &tab_id);
    let target_url: url::Url = url.parse().map_err(|e: url::ParseError| e.to_string())?;
    // When the cert bypass is on, the webview is built pointed at a harmless
    // placeholder instead of `target_url` directly and only navigated to it
    // for real afterwards (below), once `allow_self_signed_certificates` has
    // registered its handler. Building straight at `target_url` would race
    // WebView2's own initial navigation against that registration — on a fast
    // (e.g. LAN/localhost) connection the cert error can be evaluated, and
    // the interstitial shown, before our handler is attached, since both the
    // builder's implicit first navigation and our post-creation
    // `with_webview` setup are dispatched onto the same webview thread with
    // no ordering guarantee between them.
    let webview_url = if ignore_certificate_errors {
        WebviewUrl::External("about:blank".parse().unwrap())
    } else {
        WebviewUrl::External(target_url.clone())
    };

    let handle_nav = app_handle.clone();
    let app_id_nav = app_id.to_string();
    let subspace_id_nav = subspace_id.to_string();
    let tab_id_nav = tab_id.clone();

    let handle_title = app_handle.clone();
    let app_id_title = app_id.to_string();
    let subspace_id_title = subspace_id.to_string();
    let tab_id_title = tab_id.clone();

    let handle_download = app_handle.clone();
    let app_id_download = app_id.to_string();

    let handle_popup = app_handle.clone();
    let app_id_popup = app_id.to_string();
    let subspace_id_popup = subspace_id.to_string();

    // Per-subspace session isolation. `.data_directory()` maps to a real
    // separate profile on Windows (WebView2 environment) and Linux (wry keys
    // a WebKitGTK WebContext off this same path — see tauri-runtime-wry's
    // web_context handling), so one call covers both. WKWebView (macOS) has
    // no equivalent for an arbitrary path; it only isolates via
    // `.data_store_identifier([u8; 16])`, so that's added on top there — see
    // `platform::webview::mac_data_store_id` for the caveats (macOS < 14
    // silently falls back to the shared default store, i.e. NOT isolated).
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut webview_builder = WebviewBuilder::new(&label, webview_url)
        .data_directory(data_dir)
        .zoom_hotkeys_enabled(true)
        .initialization_script(NEW_TAB_INTERCEPT_SCRIPT)
        .initialization_script(ZOOM_HUD_SCRIPT);
    #[cfg(target_os = "macos")]
    {
        webview_builder = webview_builder.data_store_identifier(crate::platform::webview::mac_data_store_id(&data_slug, &session_group));
    }
    let tab_webview = window
        .add_child(
            webview_builder
                // Tauri's own drag-drop handler intercepts OS file drops before
                // WebView2 can dispatch native HTML5 drop events to page JS,
                // which breaks in-page file attach (e.g. WhatsApp Web drop zone).
                .disable_drag_drop_handler()
                .on_download(move |_webview, event| handle_download_event(&handle_download, &app_id_download, event))
                .on_navigation(move |nav_url| {
                    // The injected script redirects ctrl/shift/middle-clicked
                    // links to this fake scheme instead of letting them
                    // navigate normally, since WebView2 only reports
                    // target="_blank"/window.open() through on_new_window,
                    // not modifier-clicks on plain links.
                    if nav_url.scheme() == NEW_TAB_SCHEME {
                        if let Some((_, target)) = nav_url.query_pairs().find(|(k, _)| k == "url") {
                            let handle = handle_nav.clone();
                            let app_id = app_id_nav.clone();
                            let subspace_id = subspace_id_nav.clone();
                            let target = target.to_string();
                            std::thread::spawn(move || {
                                let store = handle.state::<Store>();
                                let registry = handle.state::<TabRegistry>();
                                let widths = handle.state::<SidebarWidths>();
                                let sidebar_width = widths.get(&app_id);
                                let _ = open_tab_internal(
                                    &handle,
                                    &store,
                                    &registry,
                                    sidebar_width,
                                    &app_id,
                                    &subspace_id,
                                    Some(target),
                                );
                                notify_toolbar(&handle, &app_id, &subspace_id);
                            });
                        }
                        return false;
                    }
                    update_tab_field(&handle_nav, &app_id_nav, &subspace_id_nav, &tab_id_nav, |t| {
                        t.url = nav_url.to_string();
                    });
                    true
                })
                .on_document_title_changed(move |_webview, title| {
                    update_tab_field(&handle_title, &app_id_title, &subspace_id_title, &tab_id_title, |t| {
                        t.title = title.clone();
                    });
                })
                // `window.open()`/`target="_blank"` requests come in two
                // flavors that need opposite handling:
                //  - real popups (OAuth/"launch" style flows, e.g.
                //    Superhuman docs' coda.io reload handoff) call
                //    `window.open(url, name, 'width=…,height=…')` with an
                //    explicit size, and rely on a genuine `window.opener`
                //    relationship to signal back and close themselves —
                //    denying and reopening as an independent tab would sever
                //    that relationship, breaking `window.opener.reload()` /
                //    `window.close()`.
                //  - plain `<a target="_blank">` clicks (no explicit size),
                //    like Google's app switcher tiles inside Gmail/Drive/
                //    Calendar — WebView2 routes these through this same
                //    callback instead of `on_navigation`, and `Allow`-ing
                //    them opens a real, separate, undecorated WebView2
                //    popup window with no controls of its own, which looks
                //    from the user's end like the click did nothing. Since
                //    there's no size to preserve a "dialog" shape, these
                //    are opened as a new Silos tab instead, same as a
                //    ctrl/shift-clicked link (see `NEW_TAB_INTERCEPT_SCRIPT`).
                .on_new_window(move |new_url, features| {
                    if features.size().is_some() {
                        return tauri::webview::NewWindowResponse::Allow;
                    }
                    let handle = handle_popup.clone();
                    let app_id = app_id_popup.clone();
                    let subspace_id = subspace_id_popup.clone();
                    std::thread::spawn(move || {
                        let store = handle.state::<Store>();
                        let registry = handle.state::<TabRegistry>();
                        let widths = handle.state::<SidebarWidths>();
                        let sidebar_width = widths.get(&app_id);
                        let _ = open_tab_internal(&handle, &store, &registry, sidebar_width, &app_id, &subspace_id, Some(new_url.to_string()));
                        notify_toolbar(&handle, &app_id, &subspace_id);
                    });
                    tauri::webview::NewWindowResponse::Deny
                }),
            LogicalPosition::new(sidebar_width, TOOLBAR_HEIGHT),
            LogicalSize::new(content_width, tab_height),
        )
        .map_err(|e| e.to_string())?;

    crate::platform::webview::setup_web_notifications(app_handle, &data_slug, app_id, &tab_webview);
    crate::platform::webview::setup_password_autosave(&tab_webview);
    crate::platform::webview::setup_zoom_indicator(&tab_webview);
    if ignore_certificate_errors {
        crate::platform::webview::allow_self_signed_certificates(&tab_webview);
        let _ = tab_webview.navigate(target_url);
    }

    {
        let mut map = registry.0.lock().unwrap();
        let entry = map.entry(subspace_key(app_id, subspace_id)).or_insert_with(SubspaceTabs::default);
        entry.tabs.push(TabInfo {
            id: tab_id.clone(),
            url,
            title: "Nuova scheda".to_string(),
        });
        entry.active = tab_id.clone();
    }
    notify_toolbar(app_handle, app_id, subspace_id);

    Ok(tab_id)
}

fn update_tab_field(app_handle: &AppHandle, app_id: &str, subspace_id: &str, tab_id: &str, mutate: impl FnOnce(&mut TabInfo)) {
    let registry = app_handle.state::<TabRegistry>();
    {
        let mut map = registry.0.lock().unwrap();
        if let Some(entry) = map.get_mut(&subspace_key(app_id, subspace_id)) {
            if let Some(tab) = entry.tabs.iter_mut().find(|t| t.id == tab_id) {
                mutate(tab);
            }
        }
    }
    notify_toolbar(app_handle, app_id, subspace_id);
}

#[tauri::command]
pub async fn open_tab(
    app_handle: AppHandle,
    store: State<'_, Store>,
    registry: State<'_, TabRegistry>,
    widths: State<'_, SidebarWidths>,
    app_id: String,
    subspace_id: String,
    url: Option<String>,
) -> Result<(), String> {
    let sidebar_width = widths.get(&app_id);
    open_tab_internal(&app_handle, &store, &registry, sidebar_width, &app_id, &subspace_id, url)?;
    Ok(())
}

#[tauri::command]
pub fn switch_tab(app_handle: AppHandle, registry: State<TabRegistry>, app_id: String, subspace_id: String, tab_id: String) -> Result<(), String> {
    let prefix = tab_prefix_for_subspace(&app_id, &subspace_id);
    for (label, webview) in app_handle.webviews() {
        if label.starts_with(&prefix) {
            let _ = webview.hide();
        }
    }

    if let Some(webview) = app_handle.get_webview(&tab_label(&app_id, &subspace_id, &tab_id)) {
        webview.show().map_err(|e| e.to_string())?;
    }

    if let Some(entry) = registry.0.lock().unwrap().get_mut(&subspace_key(&app_id, &subspace_id)) {
        entry.active = tab_id;
    }
    notify_toolbar(&app_handle, &app_id, &subspace_id);
    Ok(())
}

#[tauri::command]
pub fn close_tab(app_handle: AppHandle, registry: State<TabRegistry>, app_id: String, subspace_id: String, tab_id: String) -> Result<(), String> {
    let key = subspace_key(&app_id, &subspace_id);
    let next_active = {
        let mut map = registry.0.lock().unwrap();
        let entry = map.get_mut(&key).ok_or("subspace has no tabs")?;
        if entry.tabs.len() <= 1 {
            return Ok(());
        }
        let closing_index = entry.tabs.iter().position(|t| t.id == tab_id);
        entry.tabs.retain(|t| t.id != tab_id);
        if entry.active == tab_id {
            let fallback_index = closing_index.unwrap_or(0).min(entry.tabs.len() - 1);
            entry.active = entry.tabs[fallback_index].id.clone();
        }
        entry.active.clone()
    };

    if let Some(webview) = app_handle.get_webview(&tab_label(&app_id, &subspace_id, &tab_id)) {
        let _ = webview.close();
    }

    if let Some(webview) = app_handle.get_webview(&tab_label(&app_id, &subspace_id, &next_active)) {
        webview.show().map_err(|e| e.to_string())?;
    }

    notify_toolbar(&app_handle, &app_id, &subspace_id);
    Ok(())
}

#[tauri::command]
pub fn navigate_tab(app_handle: AppHandle, app_id: String, subspace_id: String, tab_id: String, url: String) -> Result<(), String> {
    let webview = app_handle
        .get_webview(&tab_label(&app_id, &subspace_id, &tab_id))
        .ok_or("tab not found")?;
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    webview.navigate(parsed).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn tab_back(app_handle: AppHandle, app_id: String, subspace_id: String, tab_id: String) -> Result<(), String> {
    if let Some(webview) = app_handle.get_webview(&tab_label(&app_id, &subspace_id, &tab_id)) {
        webview.eval("history.back()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn tab_forward(app_handle: AppHandle, app_id: String, subspace_id: String, tab_id: String) -> Result<(), String> {
    if let Some(webview) = app_handle.get_webview(&tab_label(&app_id, &subspace_id, &tab_id)) {
        webview.eval("history.forward()").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn reload_tab(app_handle: AppHandle, app_id: String, subspace_id: String, tab_id: String) -> Result<(), String> {
    if let Some(webview) = app_handle.get_webview(&tab_label(&app_id, &subspace_id, &tab_id)) {
        webview.reload().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Reloads the currently active tab of a subspace; used by the sidebar's
/// right-click "Ricarica pagina" action.
#[tauri::command]
pub fn reload_subspace(app_handle: AppHandle, registry: State<TabRegistry>, app_id: String, subspace_id: String) -> Result<(), String> {
    let active = registry
        .0
        .lock()
        .unwrap()
        .get(&subspace_key(&app_id, &subspace_id))
        .map(|t| t.active.clone());
    if let Some(tab_id) = active {
        if let Some(webview) = app_handle.get_webview(&tab_label(&app_id, &subspace_id, &tab_id)) {
            webview.reload().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_tabs(registry: State<TabRegistry>, app_id: String, subspace_id: String) -> Vec<TabInfo> {
    registry
        .0
        .lock()
        .unwrap()
        .get(&subspace_key(&app_id, &subspace_id))
        .map(|t| t.tabs.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_active_tab(registry: State<TabRegistry>, app_id: String, subspace_id: String) -> Option<String> {
    registry
        .0
        .lock()
        .unwrap()
        .get(&subspace_key(&app_id, &subspace_id))
        .map(|t| t.active.clone())
}

#[tauri::command]
pub fn clear_subspace_data(
    app_handle: AppHandle,
    store: State<Store>,
    registry: State<TabRegistry>,
    app_id: String,
    subspace_id: String,
) -> Result<(), String> {
    close_subspace_views(&app_handle, &registry, &app_id, &subspace_id);

    let (data_slug, session_group) = {
        let config = store.config.lock().unwrap();
        let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
        let session_group = app
            .subspaces
            .iter()
            .find(|s| s.id == subspace_id)
            .map(|s| s.session_group.clone())
            .ok_or("subspace not found")?;
        (app.data_slug.clone(), session_group)
    };

    let data_dir = safe_session_data_dir(&data_slug, &session_group)?;
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).map_err(|e| e.to_string())?;
    }

    notify_sidebar(&app_handle, &app_id);
    Ok(())
}

/// Resolves a subspace's favicon into a cached file under data/icons/ and
/// returns its path — same "resolve, don't persist yet" shape as
/// pick_local_icon/pick_macos_icon/pick_icon_from_url, so the frontend can
/// treat every icon source identically and apply whichever one it lands on
/// via `set_subspace_icon`.
#[tauri::command]
pub async fn fetch_subspace_favicon(
    app_handle: AppHandle,
    store: State<'_, Store>,
    app_id: String,
    subspace_id: String,
) -> Result<String, String> {
    let site_url = {
        let config = store.config.lock().unwrap();
        let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
        let subspace = app.subspaces.iter().find(|s| s.id == subspace_id).ok_or("subspace not found")?;
        subspace.start_url.clone().unwrap_or_else(|| app.url.clone())
    };

    let parsed = url::Url::parse(&site_url).map_err(|e| e.to_string())?;
    let favicon_url = format!(
        "{}://{}/favicon.ico",
        parsed.scheme(),
        parsed.host_str().ok_or("invalid app url")?
    );

    let response = http_client().get(&favicon_url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("favicon non trovata ({})", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("favicon vuota".into());
    }

    let dir = icons_dir(&app_handle);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(format!("resolve-{}.ico", uuid::Uuid::new_v4()));
    std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

/// Persists an already-resolved icon path (favicon fetch, local file,
/// macOSicons search, direct URL — they all just resolve to a cached file
/// under data/icons/) as a subspace's icon.
#[tauri::command]
pub fn set_subspace_icon(
    app_handle: AppHandle,
    store: State<Store>,
    app_id: String,
    subspace_id: String,
    icon_path: String,
) -> Result<(), String> {
    let mut config = store.config.lock().unwrap();
    let app = config.apps.iter_mut().find(|a| a.id == app_id).ok_or("app not found")?;
    let subspace = app.subspaces.iter_mut().find(|s| s.id == subspace_id).ok_or("subspace not found")?;
    subspace.icon = Some(icon_path);
    drop(config);
    store.save();
    notify_sidebar(&app_handle, &app_id);
    Ok(())
}

#[derive(Serialize, Clone)]
pub struct SiteInfo {
    pub url: String,
    pub title: String,
    pub icon_path: Option<String>,
}

/// Hand-picked direct URLs for common multi-word (or otherwise non-obvious)
/// web apps, checked before anything else so the omnibox-style ".com" guess
/// and the DuckDuckGo fallback don't have to get these right on their own.
/// Keys are matched case-insensitively against the trimmed query as typed.
const SITE_ALIASES: &[(&str, &str)] = &[
    ("google calendar", "https://calendar.google.com"),
    ("google docs", "https://docs.google.com"),
    ("google sheets", "https://sheets.google.com"),
    ("google slides", "https://slides.google.com"),
    ("google drive", "https://drive.google.com"),
    ("google photos", "https://photos.google.com"),
    ("google meet", "https://meet.google.com"),
    ("google maps", "https://maps.google.com"),
    ("google translate", "https://translate.google.com"),
    ("google keep", "https://keep.google.com"),
    ("google news", "https://news.google.com"),
    ("google chat", "https://chat.google.com"),
    ("google contacts", "https://contacts.google.com"),
    ("google forms", "https://forms.google.com"),
    ("google groups", "https://groups.google.com"),
    ("google voice", "https://voice.google.com"),
    ("google classroom", "https://classroom.google.com"),
    ("google earth", "https://earth.google.com"),
    ("google podcasts", "https://podcasts.google.com"),
    ("google play", "https://play.google.com"),
    ("google one", "https://one.google.com"),
    ("google ads", "https://ads.google.com"),
    ("google analytics", "https://analytics.google.com"),
    ("google tag manager", "https://tagmanager.google.com"),
    ("google cloud", "https://console.cloud.google.com"),
    ("google cloud console", "https://console.cloud.google.com"),
    ("google workspace", "https://workspace.google.com"),
    ("youtube music", "https://music.youtube.com"),
    ("youtube studio", "https://studio.youtube.com"),
    ("youtube tv", "https://tv.youtube.com"),
    ("microsoft teams", "https://teams.microsoft.com"),
    ("microsoft outlook", "https://outlook.office.com"),
    ("outlook mail", "https://outlook.office.com"),
    ("outlook", "https://outlook.office.com"),
    ("microsoft office", "https://www.office.com"),
    ("office 365", "https://www.office.com"),
    ("onedrive", "https://onedrive.live.com"),
    ("one drive", "https://onedrive.live.com"),
    ("microsoft word", "https://www.office.com/launch/word"),
    ("microsoft excel", "https://www.office.com/launch/excel"),
    ("microsoft powerpoint", "https://www.office.com/launch/powerpoint"),
    ("microsoft to do", "https://to-do.office.com"),
    ("microsoft planner", "https://tasks.office.com"),
    ("microsoft forms", "https://forms.office.com"),
    ("microsoft whiteboard", "https://whiteboard.office.com"),
    ("microsoft loop", "https://loop.microsoft.com"),
    ("copilot", "https://copilot.microsoft.com"),
    ("whatsapp", "https://web.whatsapp.com"),
    ("whatsapp web", "https://web.whatsapp.com"),
    ("telegram", "https://web.telegram.org"),
    ("telegram web", "https://web.telegram.org"),
    ("facebook messenger", "https://www.messenger.com"),
    ("messenger", "https://www.messenger.com"),
    ("prime video", "https://www.primevideo.com"),
    ("amazon prime video", "https://www.primevideo.com"),
    ("disney plus", "https://www.disneyplus.com"),
    ("apple music", "https://music.apple.com"),
    ("apple tv", "https://tv.apple.com"),
    ("icloud", "https://www.icloud.com"),
    ("proton mail", "https://mail.proton.me"),
    ("protonmail", "https://mail.proton.me"),
];

fn lookup_alias(query: &str) -> Option<&'static str> {
    let normalized = query.trim().to_lowercase();
    SITE_ALIASES.iter().find(|(k, _)| *k == normalized).map(|(_, v)| *v)
}

/// A query with no spaces is treated as a domain/URL typed directly (like a
/// browser omnibox); one with spaces needs either an alias hit or a search
/// fallback, since "https://google calendar" isn't a parseable URL.
fn looks_like_url(query: &str) -> bool {
    !query.contains(' ')
}

/// Turns a bare word/domain into a URL to try. A bare word with no dot
/// ("google") is guessed as "<word>.com", like a browser omnibox; anything
/// else is used as typed, defaulting to https:// when no scheme is given.
/// Only called once `looks_like_url` is true (no spaces).
fn normalize_query(query: &str) -> String {
    let query = query.trim();
    if query.contains("://") {
        return query.to_string();
    }
    if !query.contains('.') {
        return format!("https://{query}.com");
    }
    format!("https://{query}")
}

/// Best-effort "I'm feeling lucky" search fallback for queries that aren't a
/// direct URL/domain and aren't in SITE_ALIASES: scrapes DuckDuckGo's
/// no-JS HTML results page (no API key needed) for the first result link.
/// DuckDuckGo wraps outbound links in a redirect URL carrying the real
/// target in its `uddg` query param, which is what's actually returned.
async fn duckduckgo_search(query: &str) -> Option<url::Url> {
    let request_url = url::Url::parse_with_params("https://html.duckduckgo.com/html/", &[("q", query)]).ok()?;
    let response = http_client().get(request_url).send().await.ok()?;
    let html = response.text().await.ok()?;

    let re = regex::RegexBuilder::new(
        r#"<a\b[^>]*\bclass="result__a"[^>]*\bhref="([^"]+)"|<a\b[^>]*\bhref="([^"]+)"[^>]*\bclass="result__a""#,
    )
    .case_insensitive(true)
    .build()
    .ok()?;
    let caps = re.captures(&html)?;
    let href = caps.get(1).or_else(|| caps.get(2))?.as_str().replace("&amp;", "&");
    let href = if let Some(stripped) = href.strip_prefix("//") {
        format!("https://{stripped}")
    } else {
        href
    };

    let redirect_url = url::Url::parse(&href).ok()?;
    let real = redirect_url
        .query_pairs()
        .find(|(k, _)| k == "uddg")
        .map(|(_, v)| v.into_owned())?;
    url::Url::parse(&real).ok()
}

fn extract_title(html: &str) -> Option<String> {
    let re = regex::RegexBuilder::new(r"<title[^>]*>(.*?)</title>")
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .ok()?;
    let raw = re.captures(html)?.get(1)?.as_str().trim();
    if raw.is_empty() {
        None
    } else {
        Some(
            raw.replace("&amp;", "&")
                .replace("&quot;", "\"")
                .replace("&#39;", "'")
                .replace("&lt;", "<")
                .replace("&gt;", ">"),
        )
    }
}

/// Looks for `<link rel="icon"|"shortcut icon" href="...">` in the page head,
/// which is usually a much better icon than the bare /favicon.ico guess.
fn extract_favicon_href(html: &str, base: &url::Url) -> Option<url::Url> {
    let re = regex::RegexBuilder::new(
        r#"<link\b[^>]*\brel=["'](?:shortcut icon|icon)["'][^>]*\bhref=["']([^"']+)["'][^>]*>|<link\b[^>]*\bhref=["']([^"']+)["'][^>]*\brel=["'](?:shortcut icon|icon)["'][^>]*>"#,
    )
    .case_insensitive(true)
    .build()
    .ok()?;
    let caps = re.captures(html)?;
    let href = caps.get(1).or_else(|| caps.get(2))?.as_str();
    base.join(href).ok()
}

/// Best-effort lookup used by the dashboard/subspace "search to add" flow:
/// guesses a URL from a free-typed query, fetches the page for its <title>
/// and icon, and caches the icon under data/icons/ ready to hand to
/// create_app/create_subspace as-is.
#[tauri::command]
pub async fn resolve_site(app_handle: AppHandle, query: String) -> Result<SiteInfo, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("query vuota".into());
    }

    let candidate = if let Some(alias) = lookup_alias(query) {
        alias.to_string()
    } else if looks_like_url(query) {
        normalize_query(query)
    } else {
        match duckduckgo_search(query).await {
            Some(url) => url.to_string(),
            None => return Err("nessun sito trovato per questa ricerca".to_string()),
        }
    };
    let candidate_url = url::Url::parse(&candidate).map_err(|e| e.to_string())?;

    let response = http_client()
        .get(candidate_url.clone())
        .send()
        .await
        .map_err(|_| "sito non raggiungibile".to_string())?;
    let final_url = response.url().clone();
    let html = response.text().await.unwrap_or_default();

    // Many apps (Google Calendar/Docs/Drive, ...) redirect an unauthenticated
    // fetch to a login page on a different host. The saved URL must stay the
    // one the user actually wants (the candidate), not wherever the
    // logged-out redirect landed, and the login page's <title>/favicon
    // aren't trustworthy either — fall back to the query itself for those.
    let redirected_elsewhere = final_url.host_str() != candidate_url.host_str();

    let title = if redirected_elsewhere {
        title_case(query)
    } else {
        extract_title(&html).unwrap_or_else(|| title_case(query))
    };

    let icon_path = if redirected_elsewhere {
        let mut favicon_url = candidate_url.clone();
        favicon_url.set_path("/favicon.ico");
        favicon_url.set_query(None);
        fetch_and_cache_icon(&app_handle, favicon_url).await
    } else {
        find_page_icon(&app_handle, &final_url, &html).await
    };

    Ok(SiteInfo {
        url: candidate_url.to_string(),
        title,
        icon_path,
    })
}

fn title_case(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Re-runs just the icon lookup for a URL (page <link icon> then
/// /favicon.ico), without touching title/url — used by the "Favicon del
/// sito" retry button once the user may have edited the URL field by hand.
#[tauri::command]
pub async fn fetch_favicon_for_url(app_handle: AppHandle, url: String) -> Result<String, String> {
    let parsed = url::Url::parse(url.trim()).map_err(|e| e.to_string())?;
    let response = http_client().get(parsed.clone()).send().await.map_err(|_| "sito non raggiungibile".to_string())?;
    let final_url = response.url().clone();
    let html = response.text().await.unwrap_or_default();
    find_page_icon(&app_handle, &final_url, &html)
        .await
        .ok_or_else(|| "nessuna icona trovata sul sito".to_string())
}

async fn find_page_icon(app_handle: &AppHandle, final_url: &url::Url, html: &str) -> Option<String> {
    let icon_url = extract_favicon_href(html, final_url).unwrap_or_else(|| {
        let mut fallback = final_url.clone();
        fallback.set_path("/favicon.ico");
        fallback.set_query(None);
        fallback
    });
    fetch_and_cache_icon(app_handle, icon_url).await
}

/// Copies a user-picked icon file into data/icons/ under a fresh name, ready
/// to hand to create_app/create_subspace before either exists yet.
#[tauri::command]
pub fn pick_local_icon(app_handle: AppHandle, source_path: String) -> Result<String, String> {
    let source = std::path::Path::new(&source_path);
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    let dir = icons_dir(&app_handle);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // SVG has to be rasterized up front — every downstream consumer
    // (crop_icon, get_icon_source_info, the native window/.ico icon) is
    // built on the `image` crate, which can't decode SVG at all. Every other
    // format is still copied byte-for-byte, unchanged, to avoid a lossy
    // re-encode of a source that already decodes fine.
    if ext == "svg" {
        let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
        let img = rasterize_svg(&bytes).ok_or("SVG non valido o illeggibile")?;
        let dest = dir.join(format!("resolve-{}.png", uuid::Uuid::new_v4()));
        img.save_with_format(&dest, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        return Ok(dest.to_string_lossy().to_string());
    }

    let dest = dir.join(format!("resolve-{}.{ext}", uuid::Uuid::new_v4()));
    std::fs::copy(source, &dest).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

/// Crops a source icon (any picked source: file, favicon, macOSicons, URL) to
/// a caller-supplied pixel rect and saves the result as a fresh cached file,
/// same contract as the other icon sources — the rect is expected in the
/// source image's own pixel space (react-easy-crop's `croppedAreaPixels`
/// already reports it that way, no scaling needed on the frontend side).
#[tauri::command]
pub fn crop_icon(app_handle: AppHandle, source_path: String, x: u32, y: u32, width: u32, height: u32) -> Result<String, String> {
    let mut img = image::open(&source_path).map_err(|e| e.to_string())?;
    let (img_w, img_h) = (img.width(), img.height());
    let x = x.min(img_w.saturating_sub(1));
    let y = y.min(img_h.saturating_sub(1));
    let width = width.min(img_w - x).max(1);
    let height = height.min(img_h - y).max(1);
    let cropped = img.crop(x, y, width, height);

    let dir = icons_dir(&app_handle);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(format!("resolve-{}.png", uuid::Uuid::new_v4()));
    cropped.save(&dest).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

/// Saves a raw RGBA buffer (as read from the OS clipboard by
/// `@tauri-apps/plugin-clipboard-manager`'s `readImage()`, row-major
/// top-to-bottom) as a cached PNG, same contract as every other icon source.
/// Lets a user copy an image in the browser and paste it straight in, no
/// intermediate file needed.
#[tauri::command]
pub fn save_clipboard_image(app_handle: AppHandle, rgba: Vec<u8>, width: u32, height: u32) -> Result<String, String> {
    let buffer = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "dati immagine non validi".to_string())?;

    let dir = icons_dir(&app_handle);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(format!("resolve-{}.png", uuid::Uuid::new_v4()));
    buffer.save(&dest).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

#[derive(Serialize)]
pub struct IconSourceInfo {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub size_bytes: u64,
}

/// Reads a picked icon's real dimensions/format/file size straight off the
/// cached source file — shown next to the picker so a user can tell whether
/// what they picked is actually high-res before it gets padded/rounded down
/// into a small on-screen preview. Uses the fast dimensions-only path (no
/// full pixel decode) since this only needs metadata.
#[tauri::command]
pub fn get_icon_source_info(path: String) -> Result<IconSourceInfo, String> {
    let size_bytes = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    let reader = image::ImageReader::open(&path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let format = reader.format().map(|f| format!("{f:?}").to_uppercase()).unwrap_or_else(|| "?".to_string());
    let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
    Ok(IconSourceInfo { width, height, format, size_bytes })
}

async fn fetch_and_cache_icon(app_handle: &AppHandle, icon_url: url::Url) -> Option<String> {
    let response = http_client().get(icon_url.clone()).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let ext = std::path::Path::new(icon_url.path())
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| ["ico", "png", "jpg", "jpeg", "gif", "webp", "svg"].contains(e))
        .unwrap_or("ico")
        .to_string();
    let bytes = response.bytes().await.ok()?;
    if bytes.is_empty() {
        return None;
    }

    let dir = icons_dir(app_handle);
    std::fs::create_dir_all(&dir).ok()?;
    let dest = dir.join(format!("resolve-{}.{ext}", uuid::Uuid::new_v4()));
    std::fs::write(&dest, &bytes).ok()?;
    Some(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_macos_icons_api_key(store: State<Store>) -> Option<String> {
    store.config.lock().unwrap().macos_icons_api_key.clone()
}

#[tauri::command]
pub fn set_macos_icons_api_key(store: State<Store>, api_key: Option<String>) {
    let mut config = store.config.lock().unwrap();
    config.macos_icons_api_key = api_key.filter(|k| !k.trim().is_empty());
    drop(config);
    store.save();
}

#[derive(Deserialize)]
struct MacIconsSearchResponse {
    #[serde(default)]
    hits: Vec<MacIconHit>,
}

#[derive(Deserialize)]
struct MacIconHit {
    #[serde(rename = "appName")]
    app_name: String,
    #[serde(rename = "lowResPngUrl")]
    low_res_png_url: String,
    #[serde(default)]
    credit: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct MacIconResult {
    pub app_name: String,
    pub png_url: String,
    pub credit: Option<String>,
}

/// Searches macosicons.com's icon library (https://docs.macosicons.com/).
/// The free tier is capped at 50 queries/month, so this is only ever
/// triggered by an explicit "cerca icona" action, never automatically.
#[tauri::command]
pub async fn search_macos_icons(store: State<'_, Store>, query: String) -> Result<Vec<MacIconResult>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("query vuota".into());
    }
    let api_key = {
        let config = store.config.lock().unwrap();
        config
            .macos_icons_api_key
            .clone()
            .ok_or("Nessuna API key macOSicons impostata nelle impostazioni globali")?
    };

    let body = serde_json::json!({ "query": query, "searchOptions": { "hitsPerPage": 12 } });
    let response = http_client()
        .post("https://api.macosicons.com/api/v1/search")
        .header("x-api-key", &api_key)
        .json(&body)
        .send()
        .await
        .map_err(|_| "ricerca non riuscita".to_string())?;

    match response.status() {
        reqwest::StatusCode::UNAUTHORIZED => return Err("API key macOSicons non valida".into()),
        reqwest::StatusCode::TOO_MANY_REQUESTS => return Err("limite richieste macOSicons superato (piano free: 50/mese)".into()),
        status if !status.is_success() => return Err(format!("errore macOSicons ({status})")),
        _ => {}
    }

    let parsed: MacIconsSearchResponse = response.json().await.map_err(|e| e.to_string())?;
    Ok(parsed
        .hits
        .into_iter()
        .map(|h| MacIconResult {
            app_name: h.app_name,
            png_url: h.low_res_png_url,
            credit: h.credit,
        })
        .collect())
}

/// Downloads a chosen macOSicons search result and caches it locally, ready
/// to hand to create_app/create_subspace as-is.
#[tauri::command]
pub async fn pick_macos_icon(app_handle: AppHandle, url: String) -> Result<String, String> {
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    fetch_and_cache_trimmed_icon(&app_handle, parsed)
        .await
        .ok_or_else(|| "download icona non riuscito".to_string())
}

/// Downloads an icon from an arbitrary direct image URL the user pastes in
/// (png/jpg/gif/webp/ico — whatever the `image` crate can decode), trimmed
/// and cached the same way as the other icon sources.
#[tauri::command]
pub async fn pick_icon_from_url(app_handle: AppHandle, url: String) -> Result<String, String> {
    let parsed = url::Url::parse(url.trim()).map_err(|e| e.to_string())?;
    fetch_and_cache_trimmed_icon(&app_handle, parsed)
        .await
        .ok_or_else(|| "impossibile scaricare o decodificare l'immagine da quell'URL".to_string())
}

/// Crops away transparent padding around an image's content (bounding box
/// of pixels above an alpha threshold, plus a small breathing margin), so a
/// glyph that only fills e.g. 60% of its canvas — typical of macOS-style
/// squircle icon exports — isn't reduced to an illegible speck once scaled
/// down to a 16-24px tray/taskbar icon. A small uniform margin on all sides
/// is kept so the trimmed content doesn't touch the new canvas edge.
fn trim_transparent_padding(img: image::DynamicImage) -> image::DynamicImage {
    const ALPHA_THRESHOLD: u8 = 10;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0u32, 0u32);
    let mut found = false;

    for y in 0..height {
        for x in 0..width {
            if rgba.get_pixel(x, y)[3] > ALPHA_THRESHOLD {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if !found || (min_x == 0 && min_y == 0 && max_x == width.saturating_sub(1) && max_y == height.saturating_sub(1)) {
        return image::DynamicImage::ImageRgba8(rgba);
    }

    let content_w = max_x - min_x + 1;
    let content_h = max_y - min_y + 1;
    let margin = ((content_w.max(content_h) as f32) * 0.04).round() as u32;
    let x0 = min_x.saturating_sub(margin);
    let y0 = min_y.saturating_sub(margin);
    let x1 = (max_x + margin + 1).min(width);
    let y1 = (max_y + margin + 1).min(height);

    image::DynamicImage::ImageRgba8(image::imageops::crop_imm(&rgba, x0, y0, x1 - x0, y1 - y0).to_image())
}

/// Sniffs for an SVG payload by content, not extension/content-type — a
/// brandfetch-style CDN URL carries no reliable extension and some servers
/// mislabel the content-type, so this is the only signal worth trusting.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    String::from_utf8_lossy(head).contains("<svg")
}

/// Rasterizes an SVG to a raster `DynamicImage` the rest of the icon
/// pipeline (trim/crop/pad/resize, all built on the `image` crate) can work
/// with — `image` itself has no SVG decoder. Renders at the SVG's own aspect
/// ratio, scaled so its longer side lands at 512px: sharp enough for every
/// consumer (window icon, Start Menu .ico, in-app previews) without
/// depending on whatever nominal viewBox size the source declared.
fn rasterize_svg(bytes: &[u8]) -> Option<image::DynamicImage> {
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let longest = size.width().max(size.height()).max(1.0);
    let scale = 512.0 / longest;
    let width = (size.width() * scale).round().max(1.0) as u32;
    let height = (size.height() * scale).round().max(1.0) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let png_bytes = pixmap.encode_png().ok()?;
    image::load_from_memory(&png_bytes).ok()
}

/// Decodes any image format the `image` crate supports, falling back to SVG
/// rasterization when it doesn't (SVG is the one common icon format `image`
/// can't touch at all).
fn decode_any_image(bytes: &[u8]) -> Option<image::DynamicImage> {
    image::load_from_memory(bytes)
        .ok()
        .or_else(|| looks_like_svg(bytes).then(|| rasterize_svg(bytes)).flatten())
}

async fn fetch_and_cache_trimmed_icon(app_handle: &AppHandle, icon_url: url::Url) -> Option<String> {
    let response = http_client().get(icon_url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    if bytes.is_empty() {
        return None;
    }

    let img = decode_any_image(&bytes)?;
    let trimmed = trim_transparent_padding(img);

    let dir = icons_dir(app_handle);
    std::fs::create_dir_all(&dir).ok()?;
    let dest = dir.join(format!("resolve-{}.png", uuid::Uuid::new_v4()));
    trimmed.save_with_format(&dest, image::ImageFormat::Png).ok()?;
    Some(dest.to_string_lossy().to_string())
}

/// Resolves this webapp's own favicon (distinct from any subspace's icon)
/// into a cached file under data/icons/ and returns its path — same
/// "resolve, don't persist yet" shape as pick_local_icon/pick_macos_icon/
/// pick_icon_from_url, so the frontend can treat every icon source
/// identically and apply whichever one it lands on via `set_app_icon`.
#[tauri::command]
pub async fn fetch_app_favicon(app_handle: AppHandle, store: State<'_, Store>, app_id: String) -> Result<String, String> {
    let site_url = {
        let config = store.config.lock().unwrap();
        config
            .apps
            .iter()
            .find(|a| a.id == app_id)
            .map(|a| a.url.clone())
            .ok_or("app not found")?
    };

    let parsed = url::Url::parse(&site_url).map_err(|e| e.to_string())?;
    let favicon_url = format!(
        "{}://{}/favicon.ico",
        parsed.scheme(),
        parsed.host_str().ok_or("invalid app url")?
    );

    let response = http_client().get(&favicon_url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("favicon non trovata ({})", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("favicon vuota".into());
    }

    let dir = icons_dir(&app_handle);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(format!("resolve-{}.ico", uuid::Uuid::new_v4()));
    std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

/// Persists an already-resolved icon path (favicon fetch, local file,
/// macOSicons search, direct URL — they all just resolve to a cached file
/// under data/icons/) as a webapp's own icon, regenerating its Start Menu
/// shortcut and live window icon to match.
#[tauri::command]
pub fn set_app_icon(app_handle: AppHandle, store: State<Store>, app_id: String, icon_path: String) -> Result<(), String> {
    let mut config = store.config.lock().unwrap();
    let (app_name, icon_rounded, icon_padding, icon_background, data_slug) = {
        let app = config.apps.iter_mut().find(|a| a.id == app_id).ok_or("app not found")?;
        app.icon = Some(icon_path.clone());
        (
            app.name.clone(),
            app.icon_style.rounded,
            app.icon_style.padding_percent,
            app.icon_background_color.clone(),
            app.data_slug.clone(),
        )
    };
    drop(config);
    store.save();
    crate::shortcuts::create_shortcut(
        &app_id,
        &data_slug,
        &app_name,
        Some(&icon_path),
        icon_background.as_deref(),
        icon_padding,
        icon_rounded,
    );
    apply_window_icon(&app_handle, &app_id, Some(&icon_path), icon_rounded, icon_background.as_deref(), icon_padding);
    if let Some(tray) = app_handle.tray_by_id(&crate::layout::tray_id_for_app(&app_id)) {
        if let Some(icon) = resolve_window_icon(Some(&icon_path), icon_rounded, icon_background.as_deref(), icon_padding) {
            let _ = tray.set_icon(Some(icon));
        }
    }
    notify_sidebar(&app_handle, &app_id);
    Ok(())
}

/// Builds and pops up a native OS context menu for a subspace icon.
/// A custom HTML menu would be clipped by the sidebar webview's own
/// (narrow) bounds, so this uses a real OS popup instead, which isn't
/// constrained by any webview's surface size. Item ids are encoded as
/// "action|app_id|subspace_id[|extra]" and dispatched from a single
/// global on_menu_event handler registered in lib.rs.
#[tauri::command]
pub fn show_subspace_menu(
    app_handle: AppHandle,
    store: State<Store>,
    app_id: String,
    subspace_id: String,
) -> Result<(), String> {
    let window = app_handle
        .get_window(&app_window_label(&app_id))
        .ok_or("app window not open")?;

    let (subspace_name, display_url) = {
        let config = store.config.lock().unwrap();
        let app = config.apps.iter().find(|a| a.id == app_id).ok_or("app not found")?;
        let subspace = app
            .subspaces
            .iter()
            .find(|s| s.id == subspace_id)
            .ok_or("subspace not found")?;
        (
            subspace.name.clone(),
            subspace.start_url.clone().unwrap_or_else(|| app.url.clone()),
        )
    };

    let title_item = MenuItem::with_id(&app_handle, "header-title", &subspace_name, false, None::<&str>)
        .map_err(|e| e.to_string())?;
    let url_item = MenuItem::with_id(&app_handle, "header-url", &display_url, false, None::<&str>)
        .map_err(|e| e.to_string())?;

    let menu = MenuBuilder::new(&app_handle)
        .item(&title_item)
        .item(&url_item)
        .separator()
        .text(format!("reload|{app_id}|{subspace_id}"), "Ricarica pagina")
        .text(format!("settings|{app_id}|{subspace_id}"), "Impostazioni")
        .build()
        .map_err(|e| e.to_string())?;

    window.popup_menu(&menu).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn open_subspace_settings(
    app_handle: AppHandle,
    store: State<'_, Store>,
    app_id: String,
    subspace_id: String,
) -> Result<(), String> {
    let label = settings_window_label(&app_id, &subspace_id);

    if let Some(window) = app_handle.get_window(&label) {
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let subspace_name = {
        let config = store.config.lock().unwrap();
        config
            .apps
            .iter()
            .find(|a| a.id == app_id)
            .and_then(|a| a.subspaces.iter().find(|s| s.id == subspace_id))
            .map(|s| s.name.clone())
            .ok_or("subspace not found")?
    };

    let url = WebviewUrl::App(format!("index.html?settingsApp={app_id}&settingsSubspace={subspace_id}").into());

    tauri::WebviewWindowBuilder::new(&app_handle, &label, url)
        .title(format!("Impostazioni - {subspace_name}"))
        .inner_size(760.0, 560.0)
        .min_inner_size(560.0, 400.0)
        .background_color(APP_BG)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn read_icon_data_url(path: String) -> Result<String, String> {
    use base64::Engine;

    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let mime = mime_for_extension(ext);
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

#[cfg(test)]
mod site_search_tests {
    use super::*;

    #[test]
    fn title_and_favicon_extraction() {
        let html = r#"<html><head><title>Google</title><link rel="icon" href="/favicon.ico" type="image/x-icon"></head></html>"#;
        assert_eq!(extract_title(html).as_deref(), Some("Google"));
        let base = url::Url::parse("https://google.com/").unwrap();
        let icon = extract_favicon_href(html, &base).unwrap();
        assert_eq!(icon.as_str(), "https://google.com/favicon.ico");
    }

    #[test]
    fn normalize_bare_word() {
        assert_eq!(normalize_query("google"), "https://google.com");
        assert_eq!(normalize_query("web.whatsapp.com"), "https://web.whatsapp.com");
        assert_eq!(normalize_query("https://example.org"), "https://example.org");
    }

    #[test]
    fn alias_lookup_is_case_insensitive() {
        assert_eq!(lookup_alias("Google Calendar"), Some("https://calendar.google.com"));
        assert_eq!(lookup_alias("  google calendar  "), Some("https://calendar.google.com"));
        assert_eq!(lookup_alias("not a known alias"), None);
    }

    #[test]
    fn looks_like_url_only_without_spaces() {
        assert!(looks_like_url("google.com"));
        assert!(looks_like_url("google"));
        assert!(!looks_like_url("google calendar"));
    }

    #[test]
    fn title_case_capitalizes_each_word() {
        assert_eq!(title_case("google calendar"), "Google Calendar");
        assert_eq!(title_case("whatsapp"), "Whatsapp");
    }

    #[test]
    fn trim_transparent_padding_crops_to_content() {
        let mut img = image::RgbaImage::new(100, 100);
        for y in 40..60 {
            for x in 40..60 {
                img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let trimmed = trim_transparent_padding(image::DynamicImage::ImageRgba8(img));
        // 20x20 opaque square plus ~4% margin should end up much smaller than the 100x100 canvas.
        assert!(trimmed.width() < 30);
        assert!(trimmed.height() < 30);
    }

    #[test]
    fn trim_transparent_padding_leaves_full_bleed_image_untouched() {
        let img = image::RgbaImage::from_pixel(50, 50, image::Rgba([0, 255, 0, 255]));
        let trimmed = trim_transparent_padding(image::DynamicImage::ImageRgba8(img));
        assert_eq!((trimmed.width(), trimmed.height()), (50, 50));
    }
}
