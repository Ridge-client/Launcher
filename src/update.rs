//! Updates, as the Cairn launcher does them: Ridge asks GitHub whether a newer release is out
//! (this repository's releases only, no account, nothing sent but the request itself) and says
//! so with a dot by its version; nothing downloads or installs by itself. When the user asks,
//! the release's Setup is downloaded from this repository's own release address, checked
//! against the SHA-256 GitHub lists for it, and run silently over this install, which then
//! closes; Setup opens the new Ridge when it is done.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Published with each release by CI: a plain download, not held to the API's hourly limit.
const MANIFEST_URL: &str =
    "https://github.com/Ridge-client/Launcher/releases/latest/download/update.json";
/// The API's list of releases, for a release published before the manifest existed.
const RELEASES_URL: &str = "https://api.github.com/repos/Ridge-client/Launcher/releases?per_page=8";
/// Setup is only ever downloaded from here.
const DOWNLOAD_PREFIX: &str = "https://github.com/Ridge-client/Launcher/releases/download/";
const USER_AGENT: &str = concat!("Ridge/", env!("CARGO_PKG_VERSION"));

/// A newer release: its tag, its page, its notes, and (when GitHub lists them) its Setup's
/// address, size and SHA-256.
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub tag: String,
    pub page: String,
    pub notes: String,
    pub url: Option<String>,
    pub size: u64,
    pub sha256: Option<String>,
}

/// "0.1.2" or "v0.1.2" as four numbers, so 0.2 and 0.2.0 compare equal.
pub fn version(text: &str) -> Option<[u64; 4]> {
    let text = text.trim().trim_start_matches(['v', 'V']);
    let mut parts = [0; 4];
    let mut count = 0;
    for (i, p) in text.split('.').enumerate() {
        if i >= 4 {
            return None;
        }
        parts[i] = p.parse().ok()?;
        count += 1;
    }
    (count >= 2).then_some(parts)
}

/// The newest release in `json` (the API's list, or the manifest in its shape) when it is newer
/// than `current`.
pub fn newer(json: &str, current: &str) -> Option<Update> {
    let current = version(current)?;
    let releases: Value = serde_json::from_str(json).ok()?;
    let mut best: Option<([u64; 4], Update)> = None;
    for r in releases.as_array()? {
        if r["draft"].as_bool() == Some(true) || r["prerelease"].as_bool() == Some(true) {
            continue;
        }
        let Some(tag) = r["tag_name"].as_str() else {
            continue;
        };
        let Some(v) = version(tag) else {
            continue;
        };
        if best.as_ref().is_some_and(|(b, _)| v <= *b) {
            continue;
        }
        let setup = r["assets"].as_array().and_then(|assets| {
            assets.iter().find(|a| {
                a["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("Ridge-Setup-") && n.ends_with(".exe"))
            })
        });
        let url = setup
            .and_then(|a| a["browser_download_url"].as_str())
            // only this repository's own files
            .filter(|u| u.starts_with(DOWNLOAD_PREFIX))
            .map(str::to_string);
        let sha256 = setup
            .and_then(|a| a["digest"].as_str())
            .and_then(|d| d.strip_prefix("sha256:"))
            .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_ascii_lowercase);
        let update = Update {
            tag: tag.to_string(),
            page: r["html_url"].as_str().unwrap_or_default().to_string(),
            notes: short_notes(r["body"].as_str().unwrap_or_default()),
            url,
            size: setup.and_then(|a| a["size"].as_u64()).unwrap_or(0),
            sha256,
        };
        best = Some((v, update));
    }
    best.filter(|(v, _)| *v > current).map(|(_, u)| u)
}

/// Release notes for the update window: no checksum line, no markdown marks, about 700
/// characters at most.
pub fn short_notes(body: &str) -> String {
    let lines: Vec<String> = body
        .lines()
        .filter(|l| !l.to_ascii_lowercase().contains("sha256"))
        .map(|l| {
            l.replace("**", "")
                .replace('`', "")
                .trim_start_matches(['#', ' '])
                .to_string()
        })
        .filter(|l| !l.trim().is_empty())
        .collect();
    let text = lines.join("\n");
    if text.chars().count() <= 700 {
        text
    } else {
        let cut: String = text.chars().take(700).collect();
        format!("{}…", cut.trim_end())
    }
}

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .build()
        .map_err(|e| e.to_string())
}

async fn text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

/// Asks GitHub for a release newer than this Ridge: `Ok(None)` when this is the newest, `Err`
/// when GitHub could not be asked (offline, rate limit).
pub async fn check() -> Result<Option<Update>, String> {
    let client = client(Duration::from_secs(8))?;
    let current = env!("CARGO_PKG_VERSION");
    // the manifest when the newest release has one, else the API
    if let Ok(manifest) = text(&client, MANIFEST_URL).await {
        if serde_json::from_str::<Value>(&manifest)
            .ok()
            .and_then(|v| v.as_array().map(|a| !a.is_empty()))
            == Some(true)
        {
            return Ok(newer(&manifest, current));
        }
    }
    let list = text(&client, RELEASES_URL).await?;
    Ok(newer(&list, current))
}

/// Downloads the release's Setup to the temp folder and checks its SHA-256; `progress` gets
/// 0..1. The file is deleted when anything goes wrong.
pub async fn download(update: &Update, progress: impl Fn(f32)) -> Result<PathBuf, String> {
    let (Some(url), Some(sha256)) = (&update.url, &update.sha256) else {
        return Err("this release has no Setup file with a checksum".into());
    };
    let dir = std::env::temp_dir().join("Ridge-update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("Ridge-Setup-{}.exe", update.tag));
    let result = async {
        let client = client(Duration::from_secs(20 * 60))?;
        let mut response = client
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("the download failed ({e})"))?;
        let total = response.content_length().unwrap_or(update.size);
        let mut hasher = Sha256::new();
        let mut bytes = Vec::with_capacity(total as usize);
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("the download failed ({e})"))?
        {
            hasher.update(&chunk);
            bytes.extend_from_slice(&chunk);
            if total > 0 {
                progress(bytes.len() as f32 / total as f32);
            }
        }
        let have = format!("{:x}", hasher.finalize());
        if have != *sha256 {
            return Err("the downloaded file does not match its checksum".to_string());
        }
        std::fs::write(&file, &bytes).map_err(|e| format!("couldn't save it ({e})"))?;
        Ok(file.clone())
    }
    .await;
    if result.is_err() {
        let _ = std::fs::remove_file(&file);
    }
    result
}

/// Runs the downloaded Setup silently over this install; it opens the new Ridge when done.
/// Ridge must close right after, so Setup can replace its files.
#[cfg(windows)]
pub fn install(setup: &std::path::Path) -> Result<(), String> {
    std::process::Command::new(setup)
        .args([
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/RELAUNCH=1",
        ])
        .spawn()
        .map(drop)
        .map_err(|e| format!("couldn't start Setup ({e})"))
}

#[cfg(not(windows))]
pub fn install(_setup: &std::path::Path) -> Result<(), String> {
    Err("Ridge installs its updates on Windows only".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, digest: &str, url: &str) -> Value {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/Ridge-client/Launcher/releases/tag/{tag}"),
            "body": "- Shadows: a new tab\n- **Annotate**: zoom\nsha256 `abc`",
            "draft": false,
            "assets": [
                {"name": "update.json", "browser_download_url": "x", "size": 10},
                {"name": format!("Ridge-Setup-{tag}.exe"), "browser_download_url": url, "size": 1234, "digest": digest}
            ]
        })
    }

    const HASH: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEF";

    #[test]
    fn versions_compare_as_numbers() {
        assert_eq!(version("0.1.1"), Some([0, 1, 1, 0]));
        assert_eq!(version("v0.2"), version("0.2.0"));
        assert!(version("0.10.0") > version("0.9.9"));
        assert_eq!(version("latest"), None);
        assert_eq!(version("1"), None);
    }

    #[test]
    fn the_newest_newer_release_is_offered() {
        let url = |t: &str| format!("{DOWNLOAD_PREFIX}{t}/Ridge-Setup-{t}.exe");
        let list = Value::Array(vec![
            release("0.1.2", HASH, &url("0.1.2")),
            release("0.1.3", HASH, &url("0.1.3")),
            release("0.1.0", HASH, &url("0.1.0")),
        ])
        .to_string();
        let u = newer(&list, "0.1.1").unwrap();
        assert_eq!(u.tag, "0.1.3");
        assert_eq!(u.url.as_deref(), Some(url("0.1.3").as_str()));
        assert_eq!(u.size, 1234);
        assert_eq!(
            u.sha256.as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(u.notes, "- Shadows: a new tab\n- Annotate: zoom");
        assert_eq!(newer(&list, "0.1.3"), None);
        assert_eq!(newer(&list, "0.2.0"), None);
    }

    #[test]
    fn setup_comes_only_from_this_repository_with_a_checksum() {
        let elsewhere = Value::Array(vec![release(
            "0.2.0",
            HASH,
            "https://example.com/Ridge-Setup-0.2.0.exe",
        )])
        .to_string();
        let u = newer(&elsewhere, "0.1.1").unwrap();
        assert_eq!(u.url, None);
        let no_digest = Value::Array(vec![release(
            "0.2.0",
            "",
            &format!("{DOWNLOAD_PREFIX}0.2.0/Ridge-Setup-0.2.0.exe"),
        )])
        .to_string();
        assert_eq!(newer(&no_digest, "0.1.1").unwrap().sha256, None);
        assert_eq!(newer("not json", "0.1.1"), None);
    }

    #[test]
    fn long_notes_are_cut() {
        let long = "x".repeat(900);
        let n = short_notes(&long);
        assert_eq!(n.chars().count(), 701);
        assert!(n.ends_with('…'));
    }
}
