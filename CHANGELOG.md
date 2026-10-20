# Changelog

## [Unreleased]

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
