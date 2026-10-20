//! Self-update for the portable `silos.exe`: checks the latest GitHub release,
//! downloads and verifies the new exe, swaps it in place and restarts.
//! `tauri-plugin-updater` isn't usable here — on Windows it only supports
//! installer (NSIS/MSI) bundles, and Silos ships as a bare exe.

use serde::Serialize;
use sha2::{Digest, Sha256};

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/globdug/silos/releases/latest";
const EXE_ASSET: &str = "silos.exe";
const SHA_ASSET: &str = "silos.exe.sha256";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub notes_url: String,
}

/// A newer release plus where to download it and its checksum.
#[derive(Clone, Debug, PartialEq)]
pub struct PickedUpdate {
    pub info: UpdateInfo,
    pub exe_url: String,
    pub sha_url: String,
}

fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let core = s.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// True only when both versions parse and `latest` is strictly greater.
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parse_version(current), parse_version(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

/// Picks the update out of a GitHub "latest release" response: needs to be a
/// published, non-prerelease, strictly newer version carrying both assets.
pub fn pick_update(release: &serde_json::Value, current: &str) -> Option<PickedUpdate> {
    if release["draft"].as_bool().unwrap_or(false) || release["prerelease"].as_bool().unwrap_or(false) {
        return None;
    }
    let tag = release["tag_name"].as_str()?;
    if !is_newer(current, tag) {
        return None;
    }
    let assets = release["assets"].as_array()?;
    let url_of = |name: &str| {
        assets
            .iter()
            .find(|a| a["name"].as_str() == Some(name))
            .and_then(|a| a["browser_download_url"].as_str())
            .map(str::to_string)
    };
    Some(PickedUpdate {
        info: UpdateInfo {
            version: tag.trim_start_matches('v').to_string(),
            notes_url: release["html_url"].as_str().unwrap_or_default().to_string(),
        },
        exe_url: url_of(EXE_ASSET)?,
        sha_url: url_of(SHA_ASSET)?,
    })
}

/// Checks `bytes` against a `.sha256` file, which holds either just the hex
/// digest or `sha256sum` style `<digest>  <file name>`.
pub fn verify_sha256(bytes: &[u8], sha_text: &str) -> bool {
    let Some(expected) = sha_text.split_whitespace().next() else {
        return false;
    };
    let actual: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    actual.eq_ignore_ascii_case(expected)
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("Silos/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

/// Downloads `url` into memory, reporting `(downloaded, total)` as it goes
/// (`total` is 0 when the server doesn't say).
pub async fn download(url: &str, on_progress: impl Fn(u64, u64)) -> Result<Vec<u8>, String> {
    let mut response = client()?.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download: {}", response.status()));
    }
    let total = response.content_length().unwrap_or(0);
    let mut bytes = Vec::with_capacity(total as usize);
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        bytes.extend_from_slice(&chunk);
        on_progress(bytes.len() as u64, total);
    }
    Ok(bytes)
}

async fn fetch_latest_release() -> Result<serde_json::Value, String> {
    let response = client()?
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub: {}", response.status()));
    }
    response.json().await.map_err(|e| e.to_string())
}

/// Latest release if it's newer than the running version and installable.
pub async fn find_update() -> Result<Option<PickedUpdate>, String> {
    let release = fetch_latest_release().await?;
    Ok(pick_update(&release, current_version()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(tag: &str, draft: bool, prerelease: bool, assets: &[&str]) -> serde_json::Value {
        json!({
            "tag_name": tag,
            "draft": draft,
            "prerelease": prerelease,
            "html_url": "https://github.com/globdug/silos/releases/tag/x",
            "assets": assets.iter().map(|n| json!({
                "name": n,
                "browser_download_url": format!("https://example.test/{n}")
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn is_newer_compares_numerically() {
        assert!(is_newer("0.9.0", "0.10.0"));
        assert!(is_newer("0.2.0", "v0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.3.0", "0.2.9"));
    }

    #[test]
    fn is_newer_rejects_garbage() {
        assert!(!is_newer("0.2.0", "nightly"));
        assert!(!is_newer("x", "0.3.0"));
        assert!(!is_newer("0.2.0", "0.3"));
    }

    #[test]
    fn pick_update_returns_urls_for_newer_release() {
        let r = release("v0.3.0", false, false, &["silos.exe", "silos.exe.sha256"]);
        let picked = pick_update(&r, "0.2.0").unwrap();
        assert_eq!(picked.info.version, "0.3.0");
        assert_eq!(picked.exe_url, "https://example.test/silos.exe");
        assert_eq!(picked.sha_url, "https://example.test/silos.exe.sha256");
    }

    #[test]
    fn pick_update_needs_the_checksum_asset() {
        let r = release("v0.3.0", false, false, &["silos.exe"]);
        assert_eq!(pick_update(&r, "0.2.0"), None);
    }

    #[test]
    fn pick_update_skips_draft_prerelease_and_same_version() {
        let assets = ["silos.exe", "silos.exe.sha256"];
        assert_eq!(pick_update(&release("v0.3.0", true, false, &assets), "0.2.0"), None);
        assert_eq!(pick_update(&release("v0.3.0", false, true, &assets), "0.2.0"), None);
        assert_eq!(pick_update(&release("v0.2.0", false, false, &assets), "0.2.0"), None);
    }

    // sha256("abc")
    const ABC_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn verify_sha256_accepts_bare_and_named_digest() {
        assert!(verify_sha256(b"abc", ABC_SHA));
        assert!(verify_sha256(b"abc", &format!("{ABC_SHA}  silos.exe\n")));
        assert!(verify_sha256(b"abc", &ABC_SHA.to_uppercase()));
    }

    #[test]
    fn verify_sha256_rejects_mismatch_and_empty() {
        assert!(!verify_sha256(b"abd", ABC_SHA));
        assert!(!verify_sha256(b"abc", ""));
        assert!(!verify_sha256(b"abc", "   \n"));
    }
}
