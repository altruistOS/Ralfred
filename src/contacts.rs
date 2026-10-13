//! Contacts search. Evidence: AlfredContactsQuerier/AlfredFeatureContacts in
//! the binary; Alfred's model = a background querier surfacing people when
//! typing, with call/email/open actions per person.
//!
//! The real backend is AppleScript Contacts (needs Automation permission;
//! guarded with a hard timeout so a permission prompt can never hang us).
//! RALFRED_CONTACTS_JSON points at a JSON fixture — the query/action
//! pipeline is fully testable with it, backend swap is the same code path.

use crate::search::{Candidate, Kind};
use std::path::Path;

#[derive(serde::Deserialize)]
struct Person {
    name: String,
    #[serde(default)]
    phone: String,
    #[serde(default)]
    email: String,
}

fn fixture() -> Vec<(String, String, String)> {
    let path = match std::env::var("RALFRED_CONTACTS_JSON") {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let s = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str::<Vec<Person>>(&s)
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.name, p.phone, p.email))
        .collect()
}

/// Real backend: AppleScript Contacts, bounded so an unanswered permission
/// prompt or a wedged Contacts app can never block the router.
fn contacts_live(q: &str, max: usize) -> Vec<(String, String, String)> {
    let script = format!(
        "tell application \"Contacts\" to get {{name, value of first phone of person, value of first email of person}} of people whose name contains \"{}\"",
        q.replace('\\', "").replace('"', "")
    );
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match out {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    // bounded wait: kill after 3s so we degrade to no-results, never hang
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Vec::new();
            }
        }
    }
    let stdout = child.wait_with_output().map(|o| String::from_utf8_lossy(&o.stdout).to_string());
    match stdout {
        Ok(s) => parse_applescript_list(&s, max),
        Err(_) => Vec::new(),
    }
}

// AppleScript `get {name, phone, email} of people ...` returns a three-list
// form: {name1, name2, ...}, {phone1, ...}, {email1, ...}. We keep it simple:
// best-effort split, names first.
/// exposed for tests: parse `get {names},{phones},{emails}` AppleScript output
pub fn parse_applescript_list(s: &str, max: usize) -> Vec<(String, String, String)> {
    let s = s.trim();
    if !s.starts_with('{') || s.len() < 2 {
        return Vec::new();
    }
    // split into balanced {...} groups
    let mut groups = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '{' => {
                depth += 1;
                if depth > 1 {
                    cur.push(ch);
                }
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    groups.push(std::mem::take(&mut cur));
                } else {
                    cur.push(ch);
                }
            }
            _ if depth >= 1 => cur.push(ch),
            _ => {}
        }
    }
    if groups.is_empty() {
        return Vec::new();
    }
    let split = |g: &str| -> Vec<String> {
        g.split(", ")
            .map(|x| x.trim_matches('"').to_string())
            .collect()
    };
    let names = split(&groups[0]);
    let phones = groups.get(1).map(|g| split(g)).unwrap_or_default();
    let emails = groups.get(2).map(|g| split(g)).unwrap_or_default();
    (0..names.len().min(max))
        .map(|k| {
            (
                names[k].clone(),
                phones.get(k).cloned().unwrap_or_default(),
                emails.get(k).cloned().unwrap_or_default(),
            )
        })
        .filter(|(n, _, _)| !n.is_empty())
        .collect()
}

/// Search contacts by name substring (case-insensitive).
pub fn contacts_search(q: &str, max: usize) -> Vec<(String, String, String)> {
    if q.trim().is_empty() {
        return Vec::new();
    }
    let q = q.trim().to_lowercase();
    let rows = match std::env::var("RALFRED_CONTACTS_JSON") {
        Ok(p) => fixture(),
        Err(_) => contacts_live(&q, max),
    };
    rows.into_iter()
        .filter(|(n, _, _)| n.to_lowercase().contains(&q))
        .take(max)
        .collect()
}

/// Candidates for the router: `c <name>` keyword. Actions: tel: to call,
/// mailto: to email — mirroring Alfred's per-field contact actions.
pub fn candidates(q: &str) -> Vec<Candidate> {
    let q = q.trim();
    let (kw, rest) = match q.split_once(' ') {
        Some(x) => x,
        None => return Vec::new(),
    };
    if kw != "c" && kw != "contacts" {
        return Vec::new();
    }
    if rest.trim().is_empty() {
        return Vec::new();
    }
    let rows = contacts_search(rest.trim(), 8);
    let mut out = Vec::new();
    for (name, phone, email) in rows {
        let (sub, action) = if !phone.is_empty() {
            (format!("call {} — enter to dial", phone), format!("openurl:tel:{}", phone))
        } else if !email.is_empty() {
            (format!("email {}", email), format!("openurl:mailto:{}", email))
        } else {
            ("open contact".into(), format!("openurl:contacts?person={}", name))
        };
        out.push(Candidate {
            title: name,
            subtitle: sub,
            path: None,
            score: 830.0,
            kind: Kind::Action,
            action: Some(action),
        });
    }
    out
}
