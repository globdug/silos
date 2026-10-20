mod commands;
mod downloads;
mod layout;
mod models;
mod platform;
mod shortcuts;
mod store;

use layout::{ActiveSubspaces, BackgroundApps, LockedApps, SidebarWidths, TabRegistry};
use store::Store;
use tauri::utils::config::Color;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const APP_BG: Color = Color(24, 25, 27, 255);

/// A "--open-app <id>" CLI arg (as passed by a per-webapp Start Menu shortcut)
/// opens that app's window directly instead of the dashboard. Used both for
/// the process's own argv and for args forwarded by tauri-plugin-single-instance
/// when a shortcut is clicked while Silos is already running.
fn open_app_arg(args: &[String]) -> Option<String> {
    args.iter()
        .position(|a| a == "--open-app")
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// "(Dev)" suffix so a `tauri dev` window run alongside the built exe (see
/// `tauri.dev.conf.json`, which gives it a separate identifier so the two
/// don't fight over the single-instance lock) is visually distinguishable
/// in the taskbar.
fn dashboard_title() -> &'static str {
    if cfg!(debug_assertions) {
        "Silos (Dev)"
    } else {
        "Silos"
    }
}

pub(crate) fn show_dashboard(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_focus();
        return;
    }
    if let Ok(window) = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title(dashboard_title())
        .inner_size(1000.0, 700.0)
        .background_color(APP_BG)
        .build()
    {
        if let Some(icon) = commands::default_icon_image() {
            let _ = window.set_icon(icon);
        }
        #[cfg(windows)]
        platform::windows::winid::set_window_app_id(window.hwnd(), |f| window.run_on_main_thread(f), "Silos.Dashboard");
    }
}

fn handle_launch_args(app: &AppHandle, args: &[String]) {
    if let Some(app_id) = open_app_arg(args) {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            // Shortcut launches carry no PIN; if the app is protected this
            // errors out, so fall back to the dashboard where it can be
            // opened (and unlocked) through the normal UI flow.
            let result = commands::open_app(
                handle.clone(),
                handle.state(),
                handle.state(),
                handle.state(),
                app_id,
                None,
            )
            .await;
            if result.is_err() {
                show_dashboard(&handle);
            }
        });
    } else {
        show_dashboard(app);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Each subspace gets its own WebView2 environment (separate GPU
    // process), and running several side by side has been seen making the
    // mouse cursor go invisible over any Silos window while clicks still
    // land fine, recovering only once every Silos process is killed — a
    // known WebView2/Chromium hardware-cursor-plane conflict between
    // multiple GPU processes. Must be set before any WebView2 environment
    // is created (the first webview build), so it has to happen before
    // `tauri::Builder::default()` does anything that could create one.
    #[cfg(windows)]
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--disable-features=CalculateNativeWinOcclusion,DirectCompositionSoftwareOverlays");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            handle_launch_args(app, &args);
        }))
        // Default `open_js_links_on_click` patches every webview (including
        // tab content) to intercept target="_blank"/http(s) link clicks and
        // route them through the `opener` plugin's `open_url` IPC command
        // before `on_new_window` ever sees them. Tab webviews aren't granted
        // that command (opening arbitrary page links in the system browser
        // isn't what should happen there), so the click was silently
        // swallowed instead of falling through to our own new-window
        // handling in `commands::open_tab_internal`.
        .plugin(tauri_plugin_opener::Builder::new().open_js_links_on_click(false).build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let store = Store::load(app.handle());
            app.manage(store);
            app.manage(ActiveSubspaces::new());
            app.manage(SidebarWidths::new());
            app.manage(TabRegistry::new());
            app.manage(BackgroundApps::new());
            app.manage(LockedApps::new());
            app.manage(downloads::DownloadRegistry::new());

            let args: Vec<String> = std::env::args().collect();
            handle_launch_args(app.handle(), &args);

            app.on_menu_event(|app_handle, event| {
                let id = event.id().0.clone();
                let parts: Vec<&str> = id.split('|').collect();
                match parts.as_slice() {
                    ["reload", app_id, subspace_id] => {
                        let _ = commands::reload_subspace(
                            app_handle.clone(),
                            app_handle.state(),
                            app_id.to_string(),
                            subspace_id.to_string(),
                        );
                    }
                    ["settings", app_id, subspace_id] => {
                        let handle = app_handle.clone();
                        let app_id = app_id.to_string();
                        let subspace_id = subspace_id.to_string();
                        tauri::async_runtime::spawn(async move {
                            let _ = commands::open_subspace_settings(
                                handle.clone(),
                                handle.state(),
                                app_id,
                                subspace_id,
                            )
                            .await;
                        });
                    }
                    ["tray-show", app_id] => {
                        let _ = commands::show_background_app(app_handle.clone(), app_id.to_string());
                    }
                    ["tray-close", app_id] => {
                        commands::close_background_app(app_handle, app_id);
                    }
                    _ => {}
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_apps,
            commands::get_app_info,
            commands::create_app,
            commands::delete_app,
            commands::create_subspace,
            commands::delete_subspace,
            commands::set_subspace_session_group,
            commands::open_app,
            commands::select_subspace,
            commands::clear_subspace_data,
            commands::fetch_subspace_favicon,
            commands::set_subspace_icon,
            commands::fetch_app_favicon,
            commands::set_app_icon,
            commands::resolve_site,
            commands::fetch_favicon_for_url,
            commands::pick_local_icon,
            commands::get_macos_icons_api_key,
            commands::set_macos_icons_api_key,
            commands::search_macos_icons,
            commands::pick_macos_icon,
            commands::pick_icon_from_url,
            commands::crop_icon,
            commands::save_clipboard_image,
            commands::get_icon_source_info,
            commands::list_session_groups,
            commands::read_icon_data_url,
            commands::show_subspace_menu,
            commands::toggle_sidebar,
            commands::get_sidebar_expanded,
            commands::open_add_subspace_popup,
            commands::open_quit_confirm_popup,
            commands::open_downloads_popup,
            commands::list_downloads,
            commands::clear_downloads,
            commands::open_download,
            commands::reveal_download,
            commands::set_app_download_mode,
            commands::set_app_icon_style,
            commands::set_app_icon_background,
            commands::set_subspace_icon_background,
            commands::set_app_run_in_background,
            commands::set_app_eager_load_subspaces,
            commands::set_app_ignore_certificate_errors,
            commands::set_app_hibernate_delay_secs,
            commands::set_app_pin,
            commands::set_app_pin_lock,
            commands::unlock_app_window,
            commands::cancel_app_unlock,
            commands::show_background_app,
            commands::rename_app,
            commands::set_app_url,
            commands::reset_app_sessions,
            commands::open_app_settings,
            commands::quit_app,
            commands::open_global_settings,
            commands::open_dashboard,
            commands::rename_subspace,
            commands::reorder_subspaces,
            commands::set_subspace_start_url,
            commands::reload_subspace,
            commands::open_subspace_settings,
            commands::open_tab,
            commands::switch_tab,
            commands::close_tab,
            commands::navigate_tab,
            commands::tab_back,
            commands::tab_forward,
            commands::reload_tab,
            commands::get_tabs,
            commands::get_active_tab,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

