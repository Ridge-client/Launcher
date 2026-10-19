//! Firefox enterprise policy generation (the persona's bookmarks + privacy defaults), and the
//! start script that clears the bookmarks a profile had before.

use anyhow::Result;
use serde_json::{json, Value};

/// Where Firefox on Linux reads system-wide policies.
pub const POLICY_PATH: &str = "etc/firefox/policies/policies.json";

/// A named bookmark folder of `(title, url)` pairs.
pub struct Folder {
    pub name: String,
    pub links: Vec<(String, String)>,
}

/// Sites to check a persona hides who you are: its IP and ISP, DNS, WebRTC, timezone,
/// fingerprint, hardware and devices should all be the persona's (or its VPN's), never yours.
const INTEGRITY: [(&str, &str); 18] = [
    ("IP address and ISP", "https://browserleaks.com/ip"),
    ("ISP and location (ipinfo)", "https://ipinfo.io/"),
    ("DNS leak test", "https://www.dnsleaktest.com/"),
    (
        "DNS leak test (BrowserLeaks)",
        "https://browserleaks.com/dns",
    ),
    ("WebRTC leak test", "https://browserleaks.com/webrtc"),
    ("IP, DNS and WebRTC (ipleak.net)", "https://ipleak.net/"),
    (
        "JavaScript: timezone, language, screen",
        "https://browserleaks.com/javascript",
    ),
    ("Geolocation", "https://browserleaks.com/geo"),
    (
        "Fingerprint (Cover Your Tracks)",
        "https://coveryourtracks.eff.org/",
    ),
    (
        "Fingerprint (AmIUnique)",
        "https://amiunique.org/fingerprint",
    ),
    ("Graphics card (WebGL)", "https://browserleaks.com/webgl"),
    ("Canvas fingerprint", "https://browserleaks.com/canvas"),
    ("Fonts", "https://browserleaks.com/fonts"),
    ("Camera", "https://webcamtests.com/"),
    ("Microphone", "https://mictests.com/"),
    (
        "USB, Bluetooth and serial access",
        "https://browserleaks.com/features",
    ),
    (
        "Network: TCP/IP fingerprint",
        "https://browserleaks.com/tcp",
    ),
    ("Network: TLS fingerprint", "https://browserleaks.com/tls"),
];

const SOCIAL_MEDIA: [(&str, &str); 33] = [
    ("Facebook", "https://www.facebook.com/"),
    ("Messenger", "https://www.messenger.com/"),
    ("Instagram", "https://www.instagram.com/"),
    ("X (Twitter)", "https://x.com/"),
    ("TikTok", "https://www.tiktok.com/"),
    ("LinkedIn", "https://www.linkedin.com/"),
    ("Reddit", "https://www.reddit.com/"),
    ("YouTube", "https://www.youtube.com/"),
    ("Threads", "https://www.threads.com/"),
    ("Bluesky", "https://bsky.app/"),
    ("Mastodon", "https://mastodon.social/"),
    ("Telegram", "https://web.telegram.org/"),
    ("WhatsApp", "https://web.whatsapp.com/"),
    ("Discord", "https://discord.com/app"),
    ("Snapchat", "https://web.snapchat.com/"),
    ("Pinterest", "https://www.pinterest.com/"),
    ("Tumblr", "https://www.tumblr.com/"),
    ("Twitch", "https://www.twitch.tv/"),
    ("Kick", "https://kick.com/"),
    ("Rumble", "https://rumble.com/"),
    ("Quora", "https://www.quora.com/"),
    ("Medium", "https://medium.com/"),
    ("Nextdoor", "https://nextdoor.com/"),
    ("Flickr", "https://www.flickr.com/"),
    ("Vimeo", "https://vimeo.com/"),
    ("Truth Social", "https://truthsocial.com/"),
    ("VK", "https://vk.com/"),
    ("Odnoklassniki", "https://ok.ru/"),
    ("Weibo", "https://weibo.com/"),
    ("Douyin", "https://www.douyin.com/"),
    ("Bilibili", "https://www.bilibili.com/"),
    ("Xiaohongshu (RedNote)", "https://www.xiaohongshu.com/"),
    ("Gab", "https://gab.com/"),
];

/// The bookmarks every persona gets, as two folders on its toolbar: Integrity (ending with this
/// PC and its router, which should NOT load: `pc` and `router` are their addresses) and
/// Social media.
pub fn ridge_folders(pc: Option<&str>, router: Option<&str>) -> Vec<Folder> {
    let links = |links: &[(&str, &str)]| -> Vec<(String, String)> {
        links
            .iter()
            .map(|(n, u)| (n.to_string(), u.to_string()))
            .collect()
    };
    let mut integrity = links(&INTEGRITY);
    let local = [
        ("This PC (should NOT load)", pc),
        ("My router (should NOT load)", router),
        (
            "This PC through Docker (should NOT load)",
            Some("host.docker.internal"),
        ),
    ];
    for (name, host) in local {
        if let Some(host) = host {
            integrity.push((name.to_string(), format!("http://{host}/")));
        }
    }
    vec![
        Folder {
            name: "Integrity".into(),
            links: integrity,
        },
        Folder {
            name: "Social media".into(),
            links: links(&SOCIAL_MEDIA),
        },
    ]
}

/// Runs as root when the browser container starts, before Firefox: deletes every bookmark the
/// profile already has (Firefox's defaults, older ones), once per profile, so only Ridge's
/// folders are left. Bookmarks the persona saves afterwards are kept.
const CLEAR_BOOKMARKS_PATH: &str = "custom-cont-init.d/50-ridge-clear-bookmarks";
const CLEAR_BOOKMARKS: &str = r#"#!/bin/bash
MARK=/config/.ridge-bookmarks-cleared
[ -e "$MARK" ] && exit 0
PY=/lsiopy/bin/python3
[ -x "$PY" ] || PY=python3
find /config -name places.sqlite -not -path '*/bookmarkbackups/*' -print0 |
while IFS= read -r -d '' db; do
    "$PY" - "$db" <<'EOF'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
roots = ("root________", "menu________", "toolbar_____", "tags________", "unfiled_____", "mobile______")
db.execute("DELETE FROM moz_bookmarks WHERE guid NOT IN (%s)" % ",".join("?" * len(roots)), roots)
# Firefox's own triggers are not here to keep these counts right
db.execute("UPDATE moz_places SET foreign_count = (SELECT count(*) FROM moz_bookmarks WHERE fk = moz_places.id) + (SELECT count(*) FROM moz_keywords WHERE place_id = moz_places.id)")
db.commit()
print("cleared bookmarks in", sys.argv[1])
EOF
done
touch "$MARK"
"#;

pub fn policies_json(folders: &[Folder]) -> Value {
    let mut policies = json!({
        "DisableTelemetry": true,
        "DisableFirefoxStudies": true,
        "DisablePocket": true,
        "DontCheckDefaultBrowser": true,
        // only Ridge's own bookmarks (applies to profiles created from now on)
        "NoDefaultBookmarks": true,
    });
    // each folder straight on the toolbar; Firefox adds back any that were removed at each start
    let bookmarks: Vec<Value> = folders
        .iter()
        .flat_map(|f| {
            f.links.iter().map(|(name, url)| {
                json!({ "Title": name, "URL": url, "Folder": f.name, "Placement": "toolbar" })
            })
        })
        .collect();
    if !bookmarks.is_empty() {
        policies["Bookmarks"] = Value::Array(bookmarks);
        policies["DisplayBookmarksToolbar"] = json!(true);
    }
    json!({ "policies": policies })
}

/// Tar archive (rooted at `/`) containing the policy file, for upload into a container.
pub fn policy_archive(folders: &[Folder]) -> Result<Vec<u8>> {
    let body = serde_json::to_vec_pretty(&policies_json(folders))?;
    let mut builder = tar::Builder::new(Vec::new());

    for dir in [
        "etc/",
        "etc/firefox/",
        "etc/firefox/policies/",
        "custom-cont-init.d/",
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_mode(0o755);
        header.set_size(0);
        builder.append_data(&mut header, dir, std::io::empty())?;
    }

    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(body.len() as u64);
    builder.append_data(&mut header, POLICY_PATH, body.as_slice())?;

    let mut header = tar::Header::new_gnu();
    header.set_mode(0o755);
    header.set_size(CLEAR_BOOKMARKS.len() as u64);
    builder.append_data(
        &mut header,
        CLEAR_BOOKMARKS_PATH,
        CLEAR_BOOKMARKS.as_bytes(),
    )?;
    Ok(builder.into_inner()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_go_on_the_toolbar() {
        let folders = vec![Folder {
            name: "Search".into(),
            links: vec![("DuckDuckGo".into(), "https://duckduckgo.com".into())],
        }];
        let v = policies_json(&folders);
        let b = &v["policies"]["Bookmarks"][0];
        assert_eq!(b["URL"], "https://duckduckgo.com");
        assert_eq!(b["Folder"], "Search");
        assert_eq!(b["Placement"], "toolbar");
        assert!(v["policies"].get("ManagedBookmarks").is_none());
        assert_eq!(v["policies"]["DisableTelemetry"], true);
    }

    #[test]
    fn ridge_folders_are_integrity_and_social_media() {
        let folders = ridge_folders(Some("10.0.0.5"), Some("10.0.0.1"));
        let names: Vec<_> = folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Integrity", "Social media"]);
        for (name, url) in folders.iter().flat_map(|f| &f.links) {
            assert!(url.starts_with("http"), "{name}: {url}");
        }
        let integrity: Vec<&str> = folders[0].links.iter().map(|(_, u)| u.as_str()).collect();
        assert!(integrity.contains(&"http://10.0.0.5/") && integrity.contains(&"http://10.0.0.1/"));
        // without the addresses, only the Docker way to this PC is left
        let fewer = ridge_folders(None, None);
        assert_eq!(fewer[0].links.len(), INTEGRITY.len() + 1);
    }

    #[test]
    fn no_bookmarks_still_hardens() {
        let v = policies_json(&[]);
        assert!(v["policies"].get("Bookmarks").is_none());
        assert_eq!(v["policies"]["DisablePocket"], true);
    }

    #[test]
    fn archive_contains_policy_file() {
        let bytes = policy_archive(&[]).unwrap();
        let mut archive = tar::Archive::new(bytes.as_slice());
        let paths: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(paths.iter().any(|p| p == POLICY_PATH));
        assert!(paths.iter().any(|p| p == CLEAR_BOOKMARKS_PATH));
    }
}
