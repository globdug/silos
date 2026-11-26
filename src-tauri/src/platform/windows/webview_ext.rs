//! Windows-only WebView2 wiring: web `Notification` handling routed through
//! real Windows toasts, and password-autosave opt-in. Moved out of
//! `commands.rs` (which stays the OS-agnostic tab/window orchestration
//! layer) since these reach directly into WebView2/COM APIs.

use tauri::{AppHandle, Manager};

/// Wires up a tab's webview so its web `Notification`s actually work end to
/// end: auto-grants the permission (WebView2's own prompt UI only appears
/// for requests made from a genuine user gesture, and most sites — WhatsApp
/// Web included — request it eagerly on load, so the prompt never shows and
/// the site is left thinking the user silently denied it), then takes over
/// notification display entirely so a click on it can be routed back into
/// the app: WebView2's default handling shows the toast fine but gives the
/// host no way to learn it was clicked.
pub fn setup_web_notifications(app_handle: &AppHandle, data_slug: &str, app_id: &str, webview: &tauri::webview::Webview) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_24, COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    };
    use webview2_com::{NotificationReceivedEventHandler, PermissionRequestedEventHandler};
    use windows::core::Interface;

    let aumid = super::winid::aumid_for_slug("Silos.WebApp", data_slug);
    let app_handle = app_handle.clone();
    let app_id = app_id.to_string();

    let _ = webview.with_webview(move |pw| {
        let controller = pw.controller();
        unsafe {
            let Ok(core) = controller.CoreWebView2() else { return };

            let perm_handler = PermissionRequestedEventHandler::create(Box::new(|_sender, args| {
                if let Some(args) = args {
                    let mut kind = Default::default();
                    if args.PermissionKind(&mut kind).is_ok() && kind == COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS {
                        let _ = args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW);
                    }
                }
                Ok(())
            }));
            let mut perm_token: i64 = 0;
            let _ = core.add_PermissionRequested(&perm_handler, &mut perm_token);

            let Ok(core24) = core.cast::<ICoreWebView2_24>() else { return };
            let recv_handle = app_handle.clone();
            let recv_app_id = app_id.clone();
            let recv_aumid = aumid.clone();
            let recv_handler = NotificationReceivedEventHandler::create(Box::new(move |_sender, args| {
                let Some(args) = args else { return Ok(()) };
                args.SetHandled(true)?;
                let Ok(notification) = args.Notification() else { return Ok(()) };

                let title = read_pwstr(|out| notification.Title(out));
                let body = read_pwstr(|out| notification.Body(out));
                let icon = read_pwstr(|out| notification.IconUri(out));
                let _ = notification.ReportShown();

                let handle = recv_handle.clone();
                let app_id = recv_app_id.clone();
                let click_notification = SendableNotification(notification.clone());
                let shown = super::notifications::show_toast(
                    &recv_aumid,
                    &title,
                    &body,
                    Some(icon.as_str()).filter(|s| !s.is_empty()),
                    move || {
                        // The toast's Activated event fires on its own WinRT
                        // callback thread, not the main UI thread that owns
                        // this COM object and the app's windows — both are
                        // thread-affine on Windows, so everything reached
                        // from here has to be dispatched back to the main
                        // thread first (touching them directly from here
                        // took the whole process down).
                        let dispatch_handle = handle.clone();
                        let _ = handle.run_on_main_thread(move || {
                            click_notification.report_clicked();
                            focus_app_from_notification(&dispatch_handle, &app_id);
                        });
                    },
                );
                if shown.is_err() {
                    let _ = notification.ReportClosed();
                }
                Ok(())
            }));
            let mut recv_token: i64 = 0;
            let _ = core24.add_NotificationReceived(&recv_handler, &mut recv_token);
        }
    });
}

/// WebView2 ships with Edge's password manager, but `IsPasswordAutosaveEnabled`
/// defaults to false for embedding apps (an embedder has to opt in explicitly,
/// unlike Edge itself) — without this the "Save password?" prompt never
/// appears at all. General autofill (addresses/payment) is enabled alongside
/// it since it's controlled by the same settings interface.
pub fn setup_password_autosave(webview: &tauri::webview::Webview) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings4;
    use windows::core::Interface;

    let _ = webview.with_webview(move |pw| {
        let controller = pw.controller();
        unsafe {
            let Ok(core) = controller.CoreWebView2() else { return };
            let Ok(settings) = core.Settings() else { return };
            let Ok(settings4) = settings.cast::<ICoreWebView2Settings4>() else { return };
            let _ = settings4.SetIsPasswordAutosaveEnabled(true);
            let _ = settings4.SetIsGeneralAutofillEnabled(true);
        }
    });
}

/// Removes wry's `window.ipc` bridge from a tab's webview.
///
/// wry registers `Object.defineProperty(window, 'ipc', { value: ... })` as a
/// document-created script on every webview, which leaves a non-configurable
/// global `ipc`. Any page whose own script declares a top-level
/// `let/const/class ipc` then dies with `SyntaxError: Identifier 'ipc' has
/// already been declared` — Google Docs/Slides/Sheets do exactly that, so
/// their homescreens never initialised and showed "Problema di caricamento".
/// Tab content never talks to the host through that bridge (it has no Tauri
/// command access), so the script can go. It can't be undone from a later
/// script because the property isn't configurable; removing the registration
/// is the only way.
///
/// WebView2 numbers a webview's document-created scripts 1, 2, 3...: id "1"
/// is WebView2's own (the `window.chrome.webview` bridge) and wry registers its
/// `ipc` script right after, in `attach_ipc_handler` and ahead of every
/// initialization script, so it is always id "2". Verified against wry 0.55:
/// removing "2" leaves `window.ipc` undefined for the page while Tauri's own
/// scripts (ids 3+) stay in place. Re-check after upgrading wry/Tauri
/// (`typeof ipc` in a tab must not be a frozen object). Must run before the
/// first page loads — callers create the tab on `about:blank` and navigate
/// afterwards.
pub fn remove_wry_ipc_bridge(webview: &tauri::webview::Webview) {
    use windows::core::HSTRING;

    let _ = webview.with_webview(move |pw| {
        let controller = pw.controller();
        unsafe {
            let Ok(core) = controller.CoreWebView2() else { return };
            let _ = core.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from("2"));
        }
    });
}

/// URI of the favicon WebView2 currently has for the page loaded in this
/// webview, if it has found one yet. Reads what the browser itself resolved
/// (`<link rel=icon>`, manifest, `/favicon.ico`, redirects and login walls
/// included), so it stays right for pages a plain HTTP fetch can't reach
/// without the session cookies.
///
/// Blocks until the webview thread answers, so it must be called from a
/// worker thread (e.g. `spawn_blocking`), never from the main thread that
/// `with_webview` dispatches to.
pub fn favicon_uri(webview: &tauri::webview::Webview) -> Option<String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_15;
    use windows::core::Interface;

    let (tx, rx) = std::sync::mpsc::channel();
    let _ = webview.with_webview(move |pw| {
        let uri = unsafe {
            pw.controller()
                .CoreWebView2()
                .ok()
                .and_then(|core| core.cast::<ICoreWebView2_15>().ok())
                .map(|core15| read_pwstr(|out| core15.FaviconUri(out)))
        };
        let _ = tx.send(uri);
    });
    rx.recv_timeout(std::time::Duration::from_secs(2))
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

/// Silently accepts TLS certificate errors (expired/self-signed/hostname
/// mismatch) instead of showing WebView2's interstitial warning page. Scoped
/// to this one webview's `ICoreWebView2` — since every subspace already gets
/// its own WebView2 environment (keyed off `.data_directory()`), this never
/// leaks into other apps/subspaces that didn't opt in. Meant for apps
/// pointed at a known local/dev server with a self-signed cert; callers must
/// gate this behind an explicit per-app opt-in, never call it unconditionally.
pub fn allow_self_signed_certificates(webview: &tauri::webview::Webview) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_14, COREWEBVIEW2_SERVER_CERTIFICATE_ERROR_ACTION_ALWAYS_ALLOW,
    };
    use webview2_com::ServerCertificateErrorDetectedEventHandler;
    use windows::core::Interface;

    let _ = webview.with_webview(move |pw| {
        let controller = pw.controller();
        unsafe {
            let Ok(core) = controller.CoreWebView2() else { return };
            let Ok(core14) = core.cast::<ICoreWebView2_14>() else { return };

            let handler = ServerCertificateErrorDetectedEventHandler::create(Box::new(|_sender, args| {
                if let Some(args) = args {
                    let _ = args.SetAction(COREWEBVIEW2_SERVER_CERTIFICATE_ERROR_ACTION_ALWAYS_ALLOW);
                }
                Ok(())
            }));
            let mut token: i64 = 0;
            let _ = core14.add_ServerCertificateErrorDetected(&handler, &mut token);
        }
    });
}

/// Shows a small "123%" HUD in the bottom-right corner of a tab's page while
/// its native WebView2 zoom (ctrl+scroll / ctrl+`+`/`-`, enabled via
/// `zoom_hotkeys_enabled`) is changing, then fades it out. WebView2's zoom
/// hotkeys are handled entirely inside the browser engine with no JS-visible
/// event, so the only way to know the factor changed is this COM event on
/// the controller; the HUD itself is just a floating div injected into the
/// page (see `ZOOM_HUD_SCRIPT`) and driven via `postWebMessageAsJson` so it
/// stays in the page's own viewport (and therefore scales/repositions with
/// the zoom exactly like a real browser's would).
pub fn setup_zoom_indicator(webview: &tauri::webview::Webview) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller;
    use webview2_com::ZoomFactorChangedEventHandler;

    let eval_webview = webview.clone();
    let _ = webview.with_webview(move |pw| {
        let controller = pw.controller();
        unsafe {
            let handler = ZoomFactorChangedEventHandler::create(Box::new(move |sender, _args| {
                let Some(sender) = sender else { return Ok(()) };
                let controller: ICoreWebView2Controller = sender;
                let mut factor: f64 = 1.0;
                if controller.ZoomFactor(&mut factor).is_err() {
                    return Ok(());
                }
                let pct = (factor * 100.0).round() as i64;
                let _ = eval_webview.eval(format!("window.__silosZoomHud && window.__silosZoomHud({pct})"));
                Ok(())
            }));
            let mut token: i64 = 0;
            let _ = controller.add_ZoomFactorChanged(&handler, &mut token);
        }
    });
}

struct SendableNotification(webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Notification);
unsafe impl Send for SendableNotification {}
impl SendableNotification {
    // A plain method call (rather than a `.0` field access) forces the
    // closure below to capture this whole newtype instead of reaching
    // through to the non-Send COM interface inside it (Rust 2021's
    // disjoint-capture rules would otherwise capture just the field).
    fn report_clicked(&self) {
        let _ = unsafe { self.0.ReportClicked() };
    }
}

unsafe fn read_pwstr(f: impl FnOnce(*mut windows::core::PWSTR) -> windows::core::Result<()>) -> String {
    let mut raw = windows::core::PWSTR::null();
    if f(&mut raw).is_err() || raw.is_null() {
        return String::new();
    }
    let s = raw.to_string().unwrap_or_default();
    windows::Win32::System::Com::CoTaskMemFree(Some(raw.as_ptr() as *const _));
    s
}

/// Brings a webapp's window to front after a notification click: restores it
/// through the same path a tray click would (handles the lock overlay if
/// it's re-locked), or opens it fresh if it isn't running at all right now.
/// Must already be running on the main thread — see the dispatch in
/// `setup_web_notifications`'s `on_activated` callback, which is the only
/// caller reached from a non-main thread.
fn focus_app_from_notification(app_handle: &AppHandle, app_id: &str) {
    if app_handle.get_window(&crate::layout::app_window_label(app_id)).is_some() {
        let _ = crate::commands::show_background_app(app_handle.clone(), app_id.to_string());
        return;
    }
    let handle = app_handle.clone();
    let id = app_id.to_string();
    tauri::async_runtime::spawn(async move {
        let _ = crate::commands::open_app(handle.clone(), handle.state(), handle.state(), handle.state(), id, None).await;
    });
}
