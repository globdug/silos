use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

pub const SIDEBAR_WIDTH_COLLAPSED: f64 = 72.0;
pub const SIDEBAR_WIDTH_EXPANDED: f64 = 260.0;
pub const TOOLBAR_HEIGHT: f64 = 76.0;

/// Tracks, per app window, which subspace's toolbar/tab is currently visible.
pub struct ActiveSubspaces(pub Mutex<HashMap<String, String>>);

impl ActiveSubspaces {
    pub fn new() -> Self {
        ActiveSubspaces(Mutex::new(HashMap::new()))
    }
}

/// App ids currently hidden in the tray ("esegui in background"): their
/// window still exists (webviews alive) but isn't shown. Each gets its own
/// tray icon (id "tray-{app_id}") so it can be shown/closed individually.
pub struct BackgroundApps(pub Mutex<HashSet<String>>);

impl BackgroundApps {
    pub fn new() -> Self {
        BackgroundApps(Mutex::new(HashSet::new()))
    }
}

/// App ids that are re-locked (PIN required again) even though their window
/// still exists, e.g. backgrounded past their configured lock delay.
pub struct LockedApps(pub Mutex<HashSet<String>>);

impl LockedApps {
    pub fn new() -> Self {
        LockedApps(Mutex::new(HashSet::new()))
    }
}

pub fn tray_id_for_app(app_id: &str) -> String {
    format!("tray-{app_id}")
}

/// Tracks, per app window, the current sidebar width (collapsed/expanded).
pub struct SidebarWidths(pub Mutex<HashMap<String, f64>>);

impl SidebarWidths {
    pub fn new() -> Self {
        SidebarWidths(Mutex::new(HashMap::new()))
    }

    pub fn get(&self, app_id: &str) -> f64 {
        *self.0.lock().unwrap().get(app_id).unwrap_or(&SIDEBAR_WIDTH_COLLAPSED)
    }

    pub fn set(&self, app_id: &str, width: f64) {
        self.0.lock().unwrap().insert(app_id.to_string(), width);
    }
}

#[derive(Clone, Serialize)]
pub struct TabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
}

#[derive(Clone, Default)]
pub struct SubspaceTabs {
    pub tabs: Vec<TabInfo>,
    pub active: String,
}

/// Tracks open tabs per subspace (key = "app_id::subspace_id").
pub struct TabRegistry(pub Mutex<HashMap<String, SubspaceTabs>>);

impl TabRegistry {
    pub fn new() -> Self {
        TabRegistry(Mutex::new(HashMap::new()))
    }
}

pub fn subspace_key(app_id: &str, subspace_id: &str) -> String {
    format!("{}::{}", app_id, subspace_id)
}

pub fn app_window_label(app_id: &str) -> String {
    format!("app-{}", app_id)
}

pub fn sidebar_label(app_id: &str) -> String {
    format!("sidebar-{}", app_id)
}

pub fn toolbar_label(app_id: &str, subspace_id: &str) -> String {
    format!("toolbar-{}-{}", app_id, subspace_id)
}

pub fn toolbar_prefix(app_id: &str) -> String {
    format!("toolbar-{}-", app_id)
}

pub fn tab_label(app_id: &str, subspace_id: &str, tab_id: &str) -> String {
    format!("tab-{}-{}-{}", app_id, subspace_id, tab_id)
}

pub fn tab_prefix_for_app(app_id: &str) -> String {
    format!("tab-{}-", app_id)
}

pub fn tab_prefix_for_subspace(app_id: &str, subspace_id: &str) -> String {
    format!("tab-{}-{}-", app_id, subspace_id)
}

pub fn settings_window_label(app_id: &str, subspace_id: &str) -> String {
    format!("settings-{}-{}", app_id, subspace_id)
}

pub fn add_popup_label(app_id: &str) -> String {
    format!("addpopup-{}", app_id)
}

pub fn downloads_popup_label(app_id: &str) -> String {
    format!("downloadspopup-{}", app_id)
}

pub fn quit_confirm_label(app_id: &str) -> String {
    format!("quitconfirm-{}", app_id)
}

pub fn app_settings_window_label(app_id: &str) -> String {
    format!("appsettings-{}", app_id)
}

pub fn lock_overlay_label(app_id: &str) -> String {
    format!("lockoverlay-{}", app_id)
}

pub const GLOBAL_SETTINGS_LABEL: &str = "global-settings";
