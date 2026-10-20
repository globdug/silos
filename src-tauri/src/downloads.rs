//! In-memory list of the downloads started from each app's webviews, shown
//! in the toolbar's downloads popup. Purely a log of what `on_download`
//! reported: it never outlives the process and holds no file handles.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Serialize, Clone, Debug)]
pub struct DownloadItem {
    pub id: String,
    pub file_name: String,
    pub url: String,
    pub path: String,
    pub status: DownloadStatus,
    pub started_at: i64,
}

pub struct DownloadRegistry(pub Mutex<HashMap<String, Vec<DownloadItem>>>);

impl DownloadRegistry {
    pub fn new() -> Self {
        DownloadRegistry(Mutex::new(HashMap::new()))
    }

    /// Records a download that is about to start and returns its id.
    pub fn start(&self, app_id: &str, url: &str, path: &Path) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let item = DownloadItem {
            id: id.clone(),
            file_name: file_name_of(path),
            url: url.to_string(),
            path: path.to_string_lossy().into_owned(),
            status: DownloadStatus::InProgress,
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
        };
        self.0.lock().unwrap().entry(app_id.to_string()).or_default().push(item);
        id
    }

    /// Marks the most recent in-progress download of `url` as done. A finish
    /// for a download we never saw start is ignored.
    pub fn finish(&self, app_id: &str, url: &str, path: Option<&Path>, success: bool) {
        let mut map = self.0.lock().unwrap();
        let Some(items) = map.get_mut(app_id) else { return };
        let Some(item) = items
            .iter_mut()
            .rev()
            .find(|i| i.status == DownloadStatus::InProgress && i.url == url)
        else {
            return;
        };
        item.status = if success { DownloadStatus::Completed } else { DownloadStatus::Failed };
        if let Some(path) = path {
            item.path = path.to_string_lossy().into_owned();
            item.file_name = file_name_of(path);
        }
    }

    /// Most recent first.
    pub fn list(&self, app_id: &str) -> Vec<DownloadItem> {
        let map = self.0.lock().unwrap();
        map.get(app_id).map(|items| items.iter().rev().cloned().collect()).unwrap_or_default()
    }

    /// Drops finished and failed entries; downloads still running stay listed.
    pub fn clear(&self, app_id: &str) {
        if let Some(items) = self.0.lock().unwrap().get_mut(app_id) {
            items.retain(|i| i.status == DownloadStatus::InProgress);
        }
    }

    pub fn find(&self, app_id: &str, id: &str) -> Option<DownloadItem> {
        self.0.lock().unwrap().get(app_id)?.iter().find(|i| i.id == id).cloned()
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `dir/file_name`, or `dir/stem (N).ext` with the first free N when that
/// already exists, so a download never overwrites an earlier one.
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let as_path = Path::new(file_name);
    let stem = as_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = as_path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("unbounded range always yields a free name")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("silos-dl-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unique_path_appends_counter_when_exists() {
        let dir = scratch_dir();
        std::fs::write(dir.join("a.pdf"), b"").unwrap();
        assert_eq!(unique_path(&dir, "a.pdf"), dir.join("a (1).pdf"));
        std::fs::write(dir.join("a (1).pdf"), b"").unwrap();
        assert_eq!(unique_path(&dir, "a.pdf"), dir.join("a (2).pdf"));
    }

    #[test]
    fn unique_path_handles_no_extension() {
        let dir = scratch_dir();
        std::fs::write(dir.join("README"), b"").unwrap();
        assert_eq!(unique_path(&dir, "README"), dir.join("README (1)"));
    }

    #[test]
    fn unique_path_returns_plain_name_when_free() {
        let dir = scratch_dir();
        assert_eq!(unique_path(&dir, "b.zip"), dir.join("b.zip"));
    }

    #[test]
    fn finish_marks_matching_in_progress_item() {
        let reg = DownloadRegistry::new();
        reg.start("app", "https://x/f", Path::new("C:/d/f.bin"));
        reg.finish("app", "https://x/f", Some(Path::new("C:/d/f (1).bin")), true);
        let items = reg.list("app");
        assert_eq!(items[0].status, DownloadStatus::Completed);
        assert_eq!(items[0].file_name, "f (1).bin");
    }

    #[test]
    fn finish_failure_marks_failed() {
        let reg = DownloadRegistry::new();
        reg.start("app", "https://x/f", Path::new("C:/d/f.bin"));
        reg.finish("app", "https://x/f", None, false);
        assert_eq!(reg.list("app")[0].status, DownloadStatus::Failed);
    }

    #[test]
    fn finish_unknown_url_is_noop() {
        let reg = DownloadRegistry::new();
        reg.finish("app", "https://x/unknown", None, true);
        assert!(reg.list("app").is_empty());
    }

    #[test]
    fn clear_keeps_in_progress() {
        let reg = DownloadRegistry::new();
        reg.start("app", "https://x/a", Path::new("C:/d/a"));
        reg.start("app", "https://x/b", Path::new("C:/d/b"));
        reg.finish("app", "https://x/a", None, true);
        reg.clear("app");
        let items = reg.list("app");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url, "https://x/b");
    }
}
