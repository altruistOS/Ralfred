//! Web bookmarks search. Evidence: AlfredFeatureWebBookmarks(Config),
//! AlfredWebBookmarksManager, AlfredWebBookmarksSafariAdapter /
//! ChromeAdapter, AlfredWebBookmark — Alfred reads Safari & Chrome bookmark
//! stores and searches title+url.
//!
//! Safari's Bookmarks.plist is TCC-protected (unreadable without permission,
//! verified: plutil gets "permission to view it denied"), so the live backend
//! goes through Safari's AppleScript bookmark result (bounded 3s like all our
//! AppleScript bridges). Chrome's JSON store is plain readable when present.
//! Fixture via RALFRED_BM_JSON drives deterministic tests.

use crate::search::{Candidate, Kind};

#[derive(serde::Deserialize, Clone)]
struct Bm {
    title: String,
    url: String,
}

fn fixture() -> Vec<Bm> {
    match std::env::var("RALFRED_BM_JSON") {
        Ok(p) => std::fs::read_to_string(p)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<Bm>>(&s).ok())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Chrome bookmark JSON stores (plain files, when Chrome is installed).
fn chrome_bookmarks() -> Vec<Bm> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for prof in ["Default", "Profile 1", "Profile 2"] {
            roots.push(
                home.join(format!("Library/Application Support/Google/Chrome/{prof}/Bookmarks")),
            );
        }
    }
    let mut out = Vec::new();
    for p in roots {
        let s = match std::fs::read_to_string(&p) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&s) {
            Ok(v) => v,
            Err(_) => continue,
        };
        fn walk(v: &serde_json::Value, out: &mut Vec<Bm>) {
            if let Some(ch) = v.get("children").and_then(|c| c.as_array()) {
                for c in ch {
                    if c.get("type").and_then(|t| t.as_str()) == Some("url") {
                        if let (Some(name), Some(url)) = (
                            c.get("name").and_then(|n| n.as_str()),
                            c.get("url").and_then(|u| u.as_str()),
                        ) {
                            out.push(Bm {
                                title: name.into(),
                                url: url.into(),
                            });
                        }
                    } else {
                        walk(c, out);
                    }
                }
            }
        }
        if let Some(root) = v.get("roots") {
            walk(root, &mut out);
        }
    }
    out
}

/// Live Safari backend, bounded: AppleScript bookmarks result gives titles.
fn safari_bookmarks() -> Vec<(String, String)> {
    let script = "tell application \"Safari\" to get {name, url} of bookmarks of bookmarks folder 1";
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match out {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Vec::new();
            }
        }
    }
    let stdout = match child.wait_with_output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return Vec::new(),
    };
    // shape: {name1, name2, ...}, {url1, url2, ...} — reuse the contacts
    // parser (three-list form; url lands in the "phone" slot).
    crate::contacts::parse_applescript_list(&stdout, 512)
        .into_iter()
        .map(|(name, url, _)| (name, url))
        .filter(|(n, _)| !n.is_empty())
        .collect()
}

pub fn all_bookmarks() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> =
        fixture().into_iter().map(|b| (b.title, b.url)).collect();
    if v.is_empty() {
        v.extend(chrome_bookmarks().into_iter().map(|b| (b.title, b.url)));
        if v.is_empty() {
            v.extend(safari_bookmarks());
        }
    }
    v
}

/// `bm <text>` searches bookmarks by title or url substring.
pub fn candidates(q: &str) -> Vec<Candidate> {
    let q = q.trim();
    let (kw, rest) = match q.split_once(' ') {
        Some(x) => x,
        None => (q, ""),
    };
    if kw != "bm" && kw != "bookmark" && kw != "bookmarks" {
        return Vec::new();
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return vec![Candidate {
            title: "Web Bookmarks".into(),
            subtitle: "type `bm <text>` to search Safari/Chrome bookmarks".into(),
            path: None,
            score: 868.0,
            kind: Kind::Action,
            action: None,
        }];
    }
    let want = rest.to_lowercase();
    let mut out = Vec::new();
    for (title, url) in all_bookmarks() {
        if title.to_lowercase().contains(&want) || url.to_lowercase().contains(&want) {
            out.push(Candidate {
                title: title.clone(),
                subtitle: url.clone(),
                path: if url.is_empty() {
                    None
                } else {
                    Some(url.clone())
                },
                score: 855.0,
                kind: Kind::WebSearch,
                action: if url.is_empty() {
                    None
                } else {
                    Some(format!("openurl:{}", url))
                },
            });
        }
    }
    out.truncate(10);
    out
}
