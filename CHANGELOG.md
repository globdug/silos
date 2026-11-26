# Changelog

## [Unreleased]

## [0.4.1] - 2026-11-26

### Added
- Rebuild the Start menu shortcut of an app on demand: a button on each dashboard card (next to settings and delete) and a "Rebuild shortcut" button in the app's settings (Info → Start menu shortcut). It rewrites the shortcut from the app's current name and icon, pointing at the `silos.exe` that is running, which fixes a shortcut that went missing, shows the wrong icon, or still points at an old location after the Silos folder was moved. Previously a shortcut was only refreshed when its app was launched. On Windows it reports failure if the shortcut couldn't be written.

## [0.4.0] - 2026-11-26

### Added
- New subspace from a tab: a button in the toolbar, next to Home, opens the "New subspace" popup prefilled from the active tab. Its title becomes the name, its URL the subspace's start page, and the session defaults to shared with the subspace you're in (switchable to isolated or another group). The icon is the favicon the page itself is showing, so pages behind a login (Google Docs, Gmail, Calendar...) get their real icon. The new subspace shows up in the sidebar; the tab stays open. The favicon is read from the page on Windows; on other systems it's fetched from the site.

### Fixed
- The popup's "Favicon del sito" button, when opened from a tab, now re-reads that tab's favicon instead of fetching the site logged out, which gave Google's generic logo for any signed-in Google service. If the page can't be read and the fetch ends up on another host (a login page), no icon is picked rather than that site's.
- Icons downloaded from servers that send gzip-compressed responses without being asked (Gmail's, for one) were saved still compressed and couldn't be displayed. They're now decompressed. Icons saved before this fix stay unreadable and need to be picked again.

## [0.3.5] - 2026-11-26

### Fixed
- Google Docs, Slides and Sheets (e.g. opened from the Classroom menu) stayed blank and showed "Problema di caricamento / clear the cache" inside Silos, while working in a normal browser. Cause: the WebView2 layer injects a non-removable global `window.ipc` into every page, and Google's page script declares its own top-level `ipc`, which then fails with `SyntaxError: Identifier 'ipc' has already been declared`. Tabs no longer get that bridge (they don't use it). Windows only for now; tabs now start on a blank page and navigate to the URL right after creation.

### Changed
- Dev builds (`npm run tauri:dev`) open a Chromium remote-debugging port on each webview so tabs can be inspected over CDP. Not enabled in release builds.

## [0.3.4] - 2026-11-26

### Changed
- "Check for updates" in the dashboard footer now shows a toast when a newer version is found ("Silos X is available"), since the update banner it triggers sits at the top of the dashboard, out of sight of the footer link. It also brings back the banner if it was dismissed earlier.

### Fixed
- "Check for updates" in the dashboard footer showed "Silos is up to date." even when the check itself failed (no network, GitHub unreachable...). It now shows the error instead.

## [0.3.3] - 2026-11-26

### Fixed
- The toolbar's Home button kept going to the old start URL after changing a subspace's start URL in its settings, until the app was closed and reopened. Home now looks up the current start URL when clicked.

## [0.3.2] - 2026-10-20

### Changed
- GitHub release notes are now taken from this changelog instead of being auto-generated.

## [0.3.1] - 2026-10-20

### Changed
- Version bump only, used to test the self-updater end to end.

## [0.3.0] - 2026-10-20

### Added
- Self-update: Silos checks the latest GitHub release when it opens and every 6 hours (can be turned off in Global settings → Updates) and shows a banner on the dashboard with "Update now". There's also a manual "Check for updates" link in the dashboard footer and in Global settings. The new `silos.exe` is verified against the SHA-256 published with the release before replacing the running one; all open apps are closed for the swap and the ones that were open (including those hidden in the tray) are reopened afterwards. Needs write access to the folder `silos.exe` lives in. Releases built by CI now also publish `silos.exe.sha256`. Versions up to 0.2.0 don't have the updater, so the first update to a version with it has to be done by hand.

## [0.2.0] - 2026-10-20

### Changed
- Subspace settings (title, start URL, session sharing, icon, icon background, edge padding) now use explicit Save/Cancel buttons with a confirmation toast, like the app settings, instead of saving on blur/change.

### Added
- Downloads panel: a download button in the toolbar opens a popup listing the app's in-progress, completed and failed downloads, with "Open" and "Show in folder" actions. Per-app setting (App settings → Info → Downloads) to either save straight into the Downloads folder (default; existing files are never overwritten) or ask where to save each time.

### Fixed
- Mitigation for the mouse cursor going invisible over a Silos window (clicks still landed), which previously only recovered after closing every Silos window; it now usually comes back by moving the mouse. Disables two WebView2/Chromium compositor features (native window occlusion calculation, DirectComposition software overlays) suspected in the hardware-cursor conflict. Not a full fix: the bug also reproduces in other WebView2-based apps on Windows 11 26H2 (build 26300). Windows only for now.

## [0.1.4] - 2026-08-06

### Fixed
- Webapps pinned to the Windows 11 taskbar turned blank/white after a reboot, requiring an unpin/re-pin to fix. The Start Menu shortcut's icon file was regenerated under a fresh random name on every app launch (not just icon edits), deleting the file the taskbar pin still pointed at. The icon filename is now content-addressed (hashed from the actual icon bytes/style) so it only changes when the icon actually does.

## [0.1.3] - 2026-08-05

### Added
- Small monochrome GitHub icon next to the version/build info in the dashboard footer, linking to the repo.
- Text zoom (ctrl+scroll, ctrl+`+`/`-`) enabled in webapp tab content, with a bottom-right HUD showing the current zoom level while it changes. Windows only for now.

### Fixed
- `target="_blank"` links with no explicit popup size (e.g. Google's app switcher tiles inside Gmail/Drive/Calendar) now open as a new Silos tab instead of a real, invisible-looking WebView2 popup window that made clicks appear to do nothing.
- The `opener` plugin's default click interception, which routed every link click in tab content through an IPC command tab webviews aren't permitted to call, was silently swallowing those clicks before they could reach the app's own new-window handling. Disabled in favor of Silos's own link/new-window handling.

## [0.1.2] - 2026-08-04

### Fixed
- Popups opened via `window.open()` (e.g. OAuth/"launch" handoff flows) now open as real native windows instead of being redirected into a sidebar tab, preserving the `window.opener` relationship they rely on to reload/close themselves.

## [0.1.1] - 2026-07-27

### Added
- Footer at the bottom of the dashboard showing app version, build date, and commit hash.
- Per-app option (Security tab) to silently accept self-signed/invalid TLS certificate errors, for apps pointed at a known local/dev server. Off by default; Windows only for now.
- Multi-language UI (Italian/English) via react-i18next, with a language switcher in Global Settings. Defaults to the system language when supported, otherwise English.
- Global Settings reorganized into sections (Language, Extensions) for room to grow.

### Changed
- Global Settings navigation restructured into a sectioned sidebar, matching the per-app settings layout.
- App settings (all tabs) and Global Settings no longer autosave on blur/change: each section now has explicit Save/Cancel buttons that appear only when something changed, plus a success/error toast on save.

### Fixed
- Switching language now updates every open window (dashboard, app settings, etc.) immediately instead of only the window it was changed in.

## [0.1.0] - 2026-07-27

### Added
- First public release.
