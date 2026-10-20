import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, DownloadItem, IconSourceInfo, MacIconResult, SessionGroupInfo, SiteInfo, Subspace, TabInfo, WebApp } from "./types";

export const api = {
  listApps: () => invoke<WebApp[]>("list_apps"),

  getAppInfo: (appId: string) => invoke<AppInfo>("get_app_info", { appId }),

  createApp: (
    name: string,
    url: string,
    icon: string | null,
    iconFit: "cover" | "contain" | null = null,
    iconRounded: boolean | null = null,
    iconBackgroundColor: string | null = null,
    iconPaddingPercent: number | null = null,
  ) =>
    invoke<WebApp>("create_app", {
      name,
      url,
      icon,
      iconFit,
      iconRounded,
      iconBackgroundColor,
      iconPaddingPercent,
    }),

  deleteApp: (appId: string) => invoke<void>("delete_app", { appId }),

  createSubspace: (
    appId: string,
    name: string,
    startUrl: string | null = null,
    icon: string | null = null,
    sessionGroup: string | null = null,
    iconBackgroundColor: string | null = null,
  ) =>
    invoke<Subspace>("create_subspace", {
      appId,
      name,
      startUrl,
      icon,
      sessionGroup,
      iconBackgroundColor,
    }),

  listSessionGroups: (appId: string) => invoke<SessionGroupInfo[]>("list_session_groups", { appId }),

  resolveSite: (query: string) => invoke<SiteInfo>("resolve_site", { query }),

  fetchFaviconForUrl: (url: string) => invoke<string>("fetch_favicon_for_url", { url }),

  pickLocalIcon: (sourcePath: string) => invoke<string>("pick_local_icon", { sourcePath }),

  getMacosIconsApiKey: () => invoke<string | null>("get_macos_icons_api_key"),

  setMacosIconsApiKey: (apiKey: string | null) => invoke<void>("set_macos_icons_api_key", { apiKey }),

  searchMacosIcons: (query: string) => invoke<MacIconResult[]>("search_macos_icons", { query }),

  pickMacosIcon: (url: string) => invoke<string>("pick_macos_icon", { url }),

  pickIconFromUrl: (url: string) => invoke<string>("pick_icon_from_url", { url }),

  cropIcon: (sourcePath: string, x: number, y: number, width: number, height: number) =>
    invoke<string>("crop_icon", { sourcePath, x, y, width, height }),

  saveClipboardImage: (rgba: Uint8Array, width: number, height: number) =>
    invoke<string>("save_clipboard_image", { rgba: Array.from(rgba), width, height }),

  getIconSourceInfo: (path: string) => invoke<IconSourceInfo>("get_icon_source_info", { path }),

  openGlobalSettings: () => invoke<void>("open_global_settings"),

  openDashboard: () => invoke<void>("open_dashboard"),

  deleteSubspace: (appId: string, subspaceId: string) =>
    invoke<void>("delete_subspace", { appId, subspaceId }),

  setSubspaceSessionGroup: (appId: string, subspaceId: string, sessionGroup: string) =>
    invoke<void>("set_subspace_session_group", {
      appId,
      subspaceId,
      sessionGroup,
    }),

  openApp: (appId: string, pin: string | null = null) => invoke<void>("open_app", { appId, pin }),

  setAppPin: (appId: string, pin: string | null) => invoke<void>("set_app_pin", { appId, pin }),

  setAppPinLock: (appId: string, lockOnBackground: boolean, delaySecs: number) =>
    invoke<void>("set_app_pin_lock", { appId, lockOnBackground, delaySecs }),

  unlockAppWindow: (appId: string, pin: string) => invoke<void>("unlock_app_window", { appId, pin }),

  cancelAppUnlock: (appId: string) => invoke<void>("cancel_app_unlock", { appId }),

  selectSubspace: (appId: string, subspaceId: string) =>
    invoke<void>("select_subspace", { appId, subspaceId }),

  clearSubspaceData: (appId: string, subspaceId: string) =>
    invoke<void>("clear_subspace_data", { appId, subspaceId }),

  fetchSubspaceFavicon: (appId: string, subspaceId: string) =>
    invoke<string>("fetch_subspace_favicon", { appId, subspaceId }),

  setSubspaceIcon: (appId: string, subspaceId: string, iconPath: string) =>
    invoke<void>("set_subspace_icon", { appId, subspaceId, iconPath }),

  readIconDataUrl: (path: string) => invoke<string>("read_icon_data_url", { path }),

  showSubspaceMenu: (appId: string, subspaceId: string) =>
    invoke<void>("show_subspace_menu", { appId, subspaceId }),

  toggleSidebar: (appId: string, expanded: boolean) =>
    invoke<void>("toggle_sidebar", { appId, expanded }),

  getSidebarExpanded: (appId: string) => invoke<boolean>("get_sidebar_expanded", { appId }),

  openAddSubspacePopup: (appId: string) => invoke<void>("open_add_subspace_popup", { appId }),

  openQuitConfirmPopup: (appId: string) => invoke<void>("open_quit_confirm_popup", { appId }),

  reloadSubspace: (appId: string, subspaceId: string) =>
    invoke<void>("reload_subspace", { appId, subspaceId }),

  openSubspaceSettings: (appId: string, subspaceId: string) =>
    invoke<void>("open_subspace_settings", { appId, subspaceId }),

  renameSubspace: (appId: string, subspaceId: string, name: string) =>
    invoke<void>("rename_subspace", { appId, subspaceId, name }),

  reorderSubspaces: (appId: string, subspaceIds: string[]) =>
    invoke<void>("reorder_subspaces", { appId, subspaceIds }),

  setSubspaceStartUrl: (appId: string, subspaceId: string, startUrl: string | null) =>
    invoke<void>("set_subspace_start_url", { appId, subspaceId, startUrl }),

  setSubspaceIconBackground: (appId: string, subspaceId: string, backgroundColor: string | null) =>
    invoke<void>("set_subspace_icon_background", { appId, subspaceId, backgroundColor }),

  setAppIconStyle: (appId: string, fit: string, rounded: boolean, paddingPercent: number) =>
    invoke<void>("set_app_icon_style", { appId, fit, rounded, paddingPercent }),

  setAppIconBackground: (appId: string, backgroundColor: string | null) =>
    invoke<void>("set_app_icon_background", { appId, backgroundColor }),

  setAppRunInBackground: (appId: string, enabled: boolean) =>
    invoke<void>("set_app_run_in_background", { appId, enabled }),

  setAppEagerLoadSubspaces: (appId: string, enabled: boolean) =>
    invoke<void>("set_app_eager_load_subspaces", { appId, enabled }),

  setAppHibernateDelaySecs: (appId: string, delaySecs: number) =>
    invoke<void>("set_app_hibernate_delay_secs", { appId, delaySecs }),

  setAppDownloadMode: (appId: string, mode: "downloads" | "ask") =>
    invoke<void>("set_app_download_mode", { appId, mode }),

  listDownloads: (appId: string) => invoke<DownloadItem[]>("list_downloads", { appId }),

  clearDownloads: (appId: string) => invoke<void>("clear_downloads", { appId }),

  openDownload: (appId: string, id: string) => invoke<void>("open_download", { appId, id }),

  revealDownload: (appId: string, id: string) => invoke<void>("reveal_download", { appId, id }),

  openDownloadsPopup: (appId: string) => invoke<void>("open_downloads_popup", { appId }),

  setAppIgnoreCertificateErrors: (appId: string, enabled: boolean) =>
    invoke<void>("set_app_ignore_certificate_errors", { appId, enabled }),

  fetchAppFavicon: (appId: string) => invoke<string>("fetch_app_favicon", { appId }),

  setAppIcon: (appId: string, iconPath: string) => invoke<void>("set_app_icon", { appId, iconPath }),

  renameApp: (appId: string, name: string) => invoke<void>("rename_app", { appId, name }),

  setAppUrl: (appId: string, url: string) => invoke<void>("set_app_url", { appId, url }),

  resetAppSessions: (appId: string) => invoke<void>("reset_app_sessions", { appId }),

  openAppSettings: (appId: string) => invoke<void>("open_app_settings", { appId }),

  quitApp: (appId: string) => invoke<void>("quit_app", { appId }),

  openTab: (appId: string, subspaceId: string, url: string | null = null) =>
    invoke<void>("open_tab", { appId, subspaceId, url }),

  switchTab: (appId: string, subspaceId: string, tabId: string) =>
    invoke<void>("switch_tab", { appId, subspaceId, tabId }),

  closeTab: (appId: string, subspaceId: string, tabId: string) =>
    invoke<void>("close_tab", { appId, subspaceId, tabId }),

  navigateTab: (appId: string, subspaceId: string, tabId: string, url: string) =>
    invoke<void>("navigate_tab", { appId, subspaceId, tabId, url }),

  tabBack: (appId: string, subspaceId: string, tabId: string) =>
    invoke<void>("tab_back", { appId, subspaceId, tabId }),

  tabForward: (appId: string, subspaceId: string, tabId: string) =>
    invoke<void>("tab_forward", { appId, subspaceId, tabId }),

  reloadTab: (appId: string, subspaceId: string, tabId: string) =>
    invoke<void>("reload_tab", { appId, subspaceId, tabId }),

  getTabs: (appId: string, subspaceId: string) => invoke<TabInfo[]>("get_tabs", { appId, subspaceId }),

  getActiveTab: (appId: string, subspaceId: string) =>
    invoke<string | null>("get_active_tab", { appId, subspaceId }),
};
