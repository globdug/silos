export interface IconStyle {
  fit: "cover" | "contain";
  rounded: boolean;
  /// Inset from each edge, as a percent of the icon's size (0-40).
  padding_percent: number;
}

export interface Subspace {
  id: string;
  name: string;
  session_group: string;
  icon: string | null;
  icon_background_color: string | null;
  start_url: string | null;
}

export interface SiteInfo {
  url: string;
  title: string;
  icon_path: string | null;
}

export interface SessionGroupInfo {
  group: string;
  label: string;
}

export interface MacIconResult {
  app_name: string;
  png_url: string;
  credit: string | null;
}

export interface IconSourceInfo {
  width: number;
  height: number;
  format: string;
  size_bytes: number;
}

export interface TabInfo {
  id: string;
  url: string;
  title: string;
}

export interface WebApp {
  id: string;
  name: string;
  url: string;
  icon: string | null;
  icon_style: IconStyle;
  icon_background_color: string | null;
  data_slug: string;
  run_in_background: boolean;
  eager_load_subspaces: boolean;
  hibernate_delay_secs: number;
  has_pin: boolean;
  pin_lock_on_background: boolean;
  pin_lock_delay_secs: number;
  created_at: number;
  ignore_certificate_errors: boolean;
  download_mode: "downloads" | "ask";
  subspaces: Subspace[];
}

export interface DownloadItem {
  id: string;
  file_name: string;
  url: string;
  path: string;
  status: "in_progress" | "completed" | "failed";
  started_at: number;
}

export interface SubspaceSize {
  id: string;
  name: string;
  size_bytes: number;
}

export interface AppInfo {
  created_at: number;
  folder_path: string;
  total_size_bytes: number;
  subspaces: SubspaceSize[];
}
