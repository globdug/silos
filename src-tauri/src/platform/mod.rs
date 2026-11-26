//! OS-specific functionality lives one submodule per target OS below
//! (`windows` fully implemented; `macos`/`linux` are stubs — see their
//! `// TODO(macos)` / `// TODO(linux)` comments for what's missing). Callers
//! elsewhere in the crate go through the `notifications`/`shortcuts`/`webview`
//! facades, which resolve to the active OS's submodule via `#[cfg(target_os
//! = ...)]` re-exports — nobody outside this module needs an `#[cfg]` of
//! their own to call these.
//!
//! `winid` (Windows AppUserModelID/taskbar identity) has no facade: the
//! concept itself doesn't exist on macOS/Linux, so it's only reachable as
//! `crate::platform::windows::winid` from behind an explicit `#[cfg(windows)]`
//! at the call site, same as before this module existed.

#[cfg(windows)]
pub mod windows;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub mod linux;

// Not yet called through this facade (windows::webview_ext reaches its own
// sibling module directly) — kept for whichever future caller needs a
// stable, OS-agnostic `show_toast` path.
#[allow(unused_imports)]
pub mod notifications {
    #[cfg(windows)]
    pub use super::windows::notifications::show_toast;
    #[cfg(target_os = "macos")]
    pub use super::macos::notifications::show_toast;
    #[cfg(target_os = "linux")]
    pub use super::linux::notifications::show_toast;
}

pub mod shortcuts {
    #[cfg(windows)]
    pub use super::windows::shortcuts::{create_shortcut, remove_shortcut};
    #[cfg(target_os = "macos")]
    pub use super::macos::shortcuts::{create_shortcut, remove_shortcut};
    #[cfg(target_os = "linux")]
    pub use super::linux::shortcuts::{create_shortcut, remove_shortcut};
}

pub mod webview {
    #[cfg(windows)]
    pub use super::windows::webview_ext::{allow_self_signed_certificates, favicon_uri, remove_wry_ipc_bridge, setup_password_autosave, setup_web_notifications, setup_zoom_indicator};
    #[cfg(target_os = "macos")]
    pub use super::macos::webview_ext::{allow_self_signed_certificates, mac_data_store_id, setup_password_autosave, setup_web_notifications, setup_zoom_indicator};
    #[cfg(target_os = "linux")]
    pub use super::linux::webview_ext::{allow_self_signed_certificates, setup_password_autosave, setup_web_notifications, setup_zoom_indicator};
}
