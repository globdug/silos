use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IconStyle {
    /// "cover" (fill the square, cropping) or "contain" (show the whole
    /// original image, letterboxed).
    pub fit: String,
    pub rounded: bool,
    /// Inset from each edge, as a percent of the icon's size (0-40). Shrinks
    /// the source image and centers it, letting the background color show
    /// as a border — useful for square logos that otherwise touch the
    /// rounded-corner mask.
    #[serde(default)]
    pub padding_percent: u8,
}

impl Default for IconStyle {
    fn default() -> Self {
        IconStyle {
            fit: "cover".to_string(),
            rounded: true,
            padding_percent: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Subspace {
    pub id: String,
    pub name: String,
    /// Subspaces sharing the same session_group share cookies/storage.
    /// Defaults to the subspace's own id (fully isolated).
    pub session_group: String,
    /// Path to a locally stored icon file (favicon fetch or user-picked PNG).
    #[serde(default)]
    pub icon: Option<String>,
    /// Hex color behind the icon, or None for a transparent background.
    /// The icon's shape (fit/rounded) is set app-wide, see WebApp::icon_style.
    #[serde(default = "default_icon_background")]
    pub icon_background_color: Option<String>,
    /// Overrides the app's URL as this subspace's starting page, if set.
    #[serde(default)]
    pub start_url: Option<String>,
}

/// Single source of truth for the default icon swatch color, matching the
/// frontend's `--border`/DEFAULT_ICON_BG (App.css / constants.ts) — kept in
/// sync by hand across the Rust/TS boundary since there's no shared token
/// file between them.
pub fn default_icon_background() -> Option<String> {
    Some("#2a2a30".to_string())
}

fn default_true() -> bool {
    true
}

pub fn default_download_mode() -> String {
    "downloads".to_string()
}

fn default_hibernate_delay_secs() -> u64 {
    300
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WebApp {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub icon_style: IconStyle,
    /// Hex color behind the app icon (dashboard card, Start Menu shortcut
    /// preview), or None for transparent. Also the default a subspace's icon
    /// background falls back to display-side when it has none of its own.
    #[serde(default = "default_icon_background")]
    pub icon_background_color: Option<String>,
    /// Folder name under data/webapps/ used to store this app's session data.
    /// Fixed at creation time (slug of the name, deduplicated) and never
    /// changed by a later rename, since WebView2 locks the profile folder
    /// while a subspace is open and a live rename could fail/corrupt it.
    #[serde(default)]
    pub data_slug: String,
    /// When true, closing this app's window hides it instead of destroying
    /// it (kept alive in the system tray) rather than actually quitting.
    #[serde(default)]
    pub run_in_background: bool,
    /// When true (the default), every subspace gets its webview created
    /// (hidden, except the one actually active) as soon as the app window
    /// opens, not just the active one — so a subspace you didn't click into
    /// yet (e.g. a second WhatsApp account) still receives/shows
    /// notifications. When false, subspaces load lazily on first click
    /// instead, and switching away from one schedules it to be closed after
    /// `hibernate_delay_secs` — see that field.
    #[serde(default = "default_true")]
    pub eager_load_subspaces: bool,
    /// Only takes effect when `eager_load_subspaces` is false: seconds after
    /// switching away from a subspace before its webview(s) get closed to
    /// free memory/CPU (its last URL is remembered and reloaded fresh next
    /// time it's selected). Switching back before the delay elapses cancels
    /// it implicitly — the pending close re-checks whether the subspace is
    /// still inactive before acting.
    #[serde(default = "default_hibernate_delay_secs")]
    pub hibernate_delay_secs: u64,
    /// Hash of the PIN required to open this app, or None if unprotected.
    /// Never store the PIN itself, see `commands::hash_pin`.
    #[serde(default)]
    pub pin_hash: Option<String>,
    /// When true (and `pin_hash` is set), backgrounding this app into the
    /// tray also re-locks it: bringing it back requires the PIN again.
    #[serde(default)]
    pub pin_lock_on_background: bool,
    /// Seconds of tray-idle time before the re-lock above kicks in (0 = as
    /// soon as it's backgrounded). Ignored when `pin_lock_on_background` is
    /// false.
    #[serde(default)]
    pub pin_lock_delay_secs: u64,
    /// Unix timestamp (seconds) when this app was created. Apps loaded from
    /// a config predating this field get backfilled with the current time
    /// (see `Store::assign_missing_slugs`), so it's a lower bound for those.
    #[serde(default)]
    pub created_at: i64,
    /// When true, TLS certificate errors (expired/self-signed/hostname
    /// mismatch) in every subspace of this app are silently accepted instead
    /// of showing WebView2's interstitial warning. Off by default — meant for
    /// apps pointed at a known local/dev server with a self-signed cert, not
    /// a general-purpose bypass. Applies per-app, not globally: other apps
    /// still get normal certificate validation.
    #[serde(default)]
    pub ignore_certificate_errors: bool,
    /// What happens when a page starts a download: "downloads" saves straight
    /// into the user's Downloads folder, "ask" shows a "Save as" dialog first.
    #[serde(default = "default_download_mode")]
    pub download_mode: String,
    pub subspaces: Vec<Subspace>,
}

/// `WebApp` as sent to the frontend over IPC: same shape minus `pin_hash`,
/// which the renderer has no legitimate use for (only ever checked as a
/// bool) and shouldn't receive verbatim. `has_pin` replaces it.
#[derive(Serialize, Clone, Debug)]
pub struct WebAppView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: Option<String>,
    pub icon_style: IconStyle,
    pub icon_background_color: Option<String>,
    pub data_slug: String,
    pub run_in_background: bool,
    pub eager_load_subspaces: bool,
    pub hibernate_delay_secs: u64,
    pub has_pin: bool,
    pub pin_lock_on_background: bool,
    pub pin_lock_delay_secs: u64,
    pub created_at: i64,
    pub ignore_certificate_errors: bool,
    pub download_mode: String,
    pub subspaces: Vec<Subspace>,
}

impl From<&WebApp> for WebAppView {
    fn from(app: &WebApp) -> Self {
        WebAppView {
            id: app.id.clone(),
            name: app.name.clone(),
            url: app.url.clone(),
            icon: app.icon.clone(),
            icon_style: app.icon_style.clone(),
            icon_background_color: app.icon_background_color.clone(),
            data_slug: app.data_slug.clone(),
            run_in_background: app.run_in_background,
            eager_load_subspaces: app.eager_load_subspaces,
            hibernate_delay_secs: app.hibernate_delay_secs,
            has_pin: app.pin_hash.is_some(),
            pin_lock_on_background: app.pin_lock_on_background,
            pin_lock_delay_secs: app.pin_lock_delay_secs,
            created_at: app.created_at,
            ignore_certificate_errors: app.ignore_certificate_errors,
            download_mode: app.download_mode.clone(),
            subspaces: app.subspaces.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AppConfig {
    pub apps: Vec<WebApp>,
    /// API key for macosicons.com's icon search (https://docs.macosicons.com/),
    /// used by the icon picker in the site-search flow. Kept in the local
    /// portable config, never checked into source.
    #[serde(default)]
    pub macos_icons_api_key: Option<String>,
    /// Set when the user turns off the periodic "is there a newer Silos?"
    /// check. Stored inverted so a config without it (and `Default`) means
    /// the check is on.
    #[serde(default)]
    pub auto_update_check_disabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webapp_without_download_mode_deserializes_to_downloads() {
        let app: WebApp = serde_json::from_str(
            r#"{"id":"a","name":"A","url":"https://a.test","subspaces":[]}"#,
        )
        .unwrap();
        assert_eq!(app.download_mode, "downloads");
    }
}
