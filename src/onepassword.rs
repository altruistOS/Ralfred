//! 1Password integration. Evidence (binary): Alfred1PasswordManager/
//! Vault/Bookmark, AlfredFeature1Password(Config[+Vaults]), keyword via
//! `features.1password.keyword`, and the URL schemes actually shipped:
//!   onepassword://search/<query>
//!   onepassword://view/<vaultID>/<itemID>
//!   onepassword://open_and_fill/<vaultID>/<itemID>[/<field>]
//! Clean-room model: `op <text>` searches via the search scheme; item ids
//! (from RALFRED_OP_JSON fixture in tests) get view/open_and_fill actions.

use crate::search::{Candidate, Kind};

#[derive(serde::Deserialize)]
struct Item {
    id: String,
    #[serde(default)]
    vault: String,
    title: String,
    #[serde(default)]
    user: String,
}

fn items() -> Vec<Item> {
    match std::env::var("RALFRED_OP_JSON") {
        Ok(p) => std::fs::read_to_string(p)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<Item>>(&s).ok())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn candidates(q: &str) -> Vec<Candidate> {
    let q = q.trim();
    let (kw, rest) = match q.split_once(' ') {
        Some(x) => x,
        None => (q, ""),
    };
    if kw != "op" && kw != "1password" {
        return Vec::new();
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return vec![Candidate {
            title: "1Password".into(),
            subtitle: "type `op <query>` to search your vaults".into(),
            path: None,
            score: 870.0,
            kind: Kind::Action,
            action: None,
        }];
    }
    // item hits get precise actions; otherwise plain search-scheme hit
    let mut out = Vec::new();
    let want = rest.to_lowercase();
    for it in items() {
        if it.title.to_lowercase().contains(&want)
            || it.user.to_lowercase().contains(&want)
            || it.vault.to_lowercase().contains(&want)
        {
            out.push(Candidate {
                title: format!("{}{}", it.title, if it.user.is_empty() { String::new() } else { format!(" ({})", it.user) }),
                subtitle: format!("vault: {} · enter to open_and_fill", it.vault),
                path: None,
                score: 895.0,
                kind: Kind::Action,
                action: Some(format!("onepassword:fill/{}/{}", it.vault, it.id)),
            });
        }
    }
    if out.is_empty() {
        out.push(Candidate {
            title: format!("Search 1Password for “{}”", rest),
            subtitle: "1Password vault search".into(),
            path: None,
            score: 894.0,
            kind: Kind::Action,
            action: Some(format!("onepassword:search:{}", rest)),
        });
    }
    out
}

/// Execute `onepassword:` action tags -> the exact URL schemes from the binary.
pub fn run(tag: &str) -> std::io::Result<()> {
    let body = tag.strip_prefix("onepassword:").unwrap_or(tag);
    let url = if let Some(q) = body.strip_prefix("search:") {
        format!("onepassword://search/{}", urlencode(q))
    } else if let Some(pair) = body.strip_prefix("fill/") {
        let (v, i) = pair.split_once('/').unwrap_or((pair, ""));
        format!("onepassword://open_and_fill/{}/{}", v, i)
    } else if let Some(pair) = body.strip_prefix("view/") {
        format!("onepassword://view/{}", pair)
    } else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad op action"));
    };
    let _ = crate::router::exec_action(crate::router::Action::OpenUrl(url));
    Ok(())
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "%20".into(),
            _ => format!("%{:02X}", b),
        })
        .collect()
}
