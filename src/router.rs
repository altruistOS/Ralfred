//! Action router — decides what kind of results a query produces, equivalent
//! to AlfredFeatureFactory: calculator > snippet keyword > workflow keyword >
//! web-search keyword > default results > clipboard > url history > docs.

use crate::calc::calculate;
use crate::config::Config;
use crate::db::Store;
use crate::search::{default_results, Candidate, Kind};
use crate::workflow::load_workflows;
use std::path::PathBuf;
use std::process::Command;

pub enum Action {
    Open(PathBuf),
    OpenUrl(String),
    CopyText(String),
    Paste(String),
    SnippetText(String),
    Reveal(PathBuf),
    OpenTerminal(PathBuf),
    None,
}

pub fn route(query: &str, cfg: &Config, store: &Store) -> Vec<Candidate> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }

    // 1. calculator
    if crate::calc::is_calc(q) && crate::features::enabled("calculator") {
        if let Some(r) = calculate(q) {
            return vec![Candidate {
                title: r,
                subtitle: "type to copy the result".into(),
                path: None,
                score: 1000.0,
                kind: Kind::Calc,
                action: None,
            }];
        }
    }

    let mut results: Vec<Candidate> = Vec::new();

    // 2. snippet keyword expansion (keyword + space + arg)
    if crate::features::enabled("snippets") {
      if let Some((kw, rest)) = q.split_once(' ') {
        let snips = Store::snippets();
        for s in snips.find_snippets(kw) {
            if s.keyword == kw {
                let mut text = s.text.clone();
                text = text.replace("{query}", rest).replace("{q}", rest);
                results.push(Candidate {
                    title: format!("{} → {}", s.keyword, s.name),
                    subtitle: text.clone().lines().next().unwrap_or("").chars().take(60).collect(),
                    path: Some(text),
                    score: 950.0,
                    kind: Kind::Snippet,
                    action: None,
                });
            }
        }
      }
    }

    // 3. workflow keyword match (full graph execution)
    let wfs = load_workflows(&crate::config::workflows_dir());
    let wfs = if crate::features::enabled("workflows") { wfs } else { Vec::new() };
    let (maybe_kw, _rest) = match q.split_once(' ') {
        Some((a, b)) => (a.to_string(), b),
        None => (q.to_string(), ""),
    };
    for wf in &wfs {
        if let Some(kw) = &wf.keyword {
            if *kw == maybe_kw || q.starts_with(kw) {
                let rest = if q.starts_with(kw) {
                    q[kw.len()..].trim_start()
                } else {
                    ""
                };
                let mut items = wf.run_keyword(rest);
                for it in &mut items {
                    if it.subtitle.is_empty() {
                        it.subtitle = format!("[{}]", wf.name);
                    } else {
                        it.subtitle = format!("{} · {}", it.subtitle, wf.name);
                    }
                    it.score = 900.0;
                }
                results.append(&mut items);
            }
        }
    }

    // 4. web search keyword
    for (k, tmpl) in &cfg.web_search {
        if q == *k || q.starts_with(&format!("{k} ")) {
            let query = if q == *k { "" } else { &q[k.len() + 1..] };
            let url = tmpl.replace("{query}", &url_encode(query));
            results.push(Candidate {
                title: format!("Search {} for '{}'", k, query),
                subtitle: url.clone(),
                path: Some(url),
                score: 850.0,
                kind: Kind::WebSearch,
                action: None,
            });
        }
    }

    // 4a. contacts (evidence: AlfredContactsQuerier + AlfredFeatureContacts).
    // `c <name>` / `contacts <name>`; fixture-backed in tests via RALFRED_CONTACTS_JSON.
    if crate::features::enabled("contacts") { results.extend(crate::contacts::candidates(q)); }

    // 4a1. 1password (evidence: Alfred1PasswordManager + onepassword:// URL schemes)
    if crate::features::enabled("onepassword") { results.extend(crate::onepassword::candidates(q)); }

    // 4a14. large type (evidence: AlfredFeatureLargeType, AlfredLargeTypeView)
    if crate::features::enabled("largetype") { results.extend(crate::largetype::candidates(q)); }

    // 4a15. web bookmarks (evidence: AlfredFeatureWebBookmarks, Safari/Chrome adapters)
    if crate::features::enabled("bookmarks") { results.extend(crate::webbookmarks::candidates(q)); }

    // 4a2. itunes/music (evidence: AlfredFeatureiTunes, alfred.itunes.play/.random)
    if crate::features::enabled("music") { results.extend(crate::itunes::candidates(q)); }

    // 4b. dictionary lookup (macOS dict:// URL scheme; evidence: AlfredFeatureDictionary
    // + dict://{query} string in binary). `dict <word>` mirrors Alfred.
    if crate::features::enabled("dictionary") && (q == "dict" || q.starts_with("dict ")) {
        let term = q[4..].trim();
        if !term.is_empty() {
            let url = format!("dict://{}", url_encode(term));
            results.push(Candidate {
                title: format!("{} — dictionary lookup", term),
                subtitle: "opens macOS Dictionary · enter to define".into(),
                path: Some(url),
                score: 860.0,
                kind: Kind::Dictionary,
                action: None,
            });
        }
    }

    // 5. default results (apps / pref panes)
    if crate::features::enabled("filesearch") {
        let mut dr = default_results(q, cfg, store, cfg.max_results);
        for r in &mut dr {
            r.score += 1.0; // slightly below keyword hits
        }
        results.append(&mut dr);

        // 6. documents via mdfind (filename/content) — Alfred's file-search backend
        if !q.contains('/') {
            let docs = docs_search(q, 8);
            results.extend(docs);
        }
    }

    // 7. clipboard search when prefixed with ','
    if let Some(rest) = q.strip_prefix(',') {
        if !rest.trim().is_empty() && crate::features::enabled("clipboard") {
            for (item, _) in Store::clipboard().clipboard_search(rest.trim(), 20) {
                results.push(Candidate {
                    title: item.clone().lines().next().unwrap_or("").chars().take(70).collect(),
                    subtitle: "clipboard — enter to paste".into(),
                    path: Some(item),
                    score: 700.0,
                    kind: Kind::Clipboard,
                    action: None,
                });
            }
        }
    } else {
        // url history fallback
        let hist = Store::urlhistory().url_history(q, 5);
        for (item, _) in hist {
            results.push(Candidate {
                title: item.clone(),
                subtitle: "URL history".into(),
                path: Some(item),
                score: 50.0,
                kind: Kind::WebSearch,
                action: None,
            });
        }
    }

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    results.truncate(cfg.max_results);
    results
}

/// File/document search. Primary backend is our own in-memory file index
/// (self-built like Alfred, scoped to user-designated roots) — microseconds
/// per keystroke, no Spotlight, no disk per keystroke. Content-mode Spotlight
/// is only ever a rate-limited fallback (5s budget) when the index misses.
pub fn docs_search(q: &str, max: usize) -> Vec<Candidate> {
    // name matching via the scope index — matches Alfred, whose file search
    // is name-based; Spotlight content queries stay out of the keystroke
    // path (opt in: RALFRED_CONTENT_SEARCH=1).
    let out = crate::index::docs_candidates(q, max);
    if out.is_empty() && std::env::var("RALFRED_CONTENT_SEARCH").is_ok() {
        return crate::index::mdfind_fallback(q, max);
    }
    out
}

pub fn path_candidates(q: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    if q.starts_with('/') || q.starts_with("~/") {
        let expanded = crate::config::expand_home(q);
        let parent = expanded.parent().unwrap_or(&expanded);
        let stem = expanded
            .file_name()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if let Ok(rd) = std::fs::read_dir(parent) {
            for e in rd.flatten().take(50) {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if !stem.is_empty() && name.starts_with(&stem) {
                    let p = e.path();
                    out.push(Candidate {
                        title: e.file_name().to_string_lossy().to_string(),
                        subtitle: p.to_string_lossy().to_string(),
                        path: Some(p.to_string_lossy().to_string()),
                        score: 300.0,
                        kind: if p.is_dir() { Kind::Dir } else { Kind::File },
                        action: None,
                    });
                }
            }
        }
    }
    out
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn exec_action(a: Action) -> std::io::Result<()> {
    // harness/test guard: never touch the real clipboard, never launch apps
    if std::env::var("RALFRED_NOACT").is_ok_and(|v| v == "1") {
        return Ok(());
    }
    match a {
        Action::Open(p) => {
            Command::new("open").arg(&p).status()?;
            Ok(())
        }
        Action::OpenUrl(u) => {
            Command::new("open").arg(&u).status()?;
            Ok(())
        }
        Action::CopyText(t) => set_clipboard(&t),
        Action::Paste(t) => {
            set_clipboard(&t)?;
            Command::new("osascript")
                .arg("-e")
                .arg("tell application \"System Events\" to keystroke \"v\" using command down")
                .status()?;
            Ok(())
        }
        Action::SnippetText(t) => {
            set_clipboard(&t)?;
            Command::new("osascript")
                .arg("-e")
                .arg("tell application \"System Events\" to keystroke \"v\" using command down")
                .status()?;
            Ok(())
        }
        Action::Reveal(p) => {
            Command::new("open").arg("-R").arg(&p).status()?;
            Ok(())
        }
        Action::OpenTerminal(dir) => {
            Command::new("open").arg("-a").arg("Terminal").arg(dir).status()?;
            Ok(())
        }
        Action::None => Ok(()),
    }
}

pub fn set_clipboard(text: &str) -> std::io::Result<()> {
    let p = "/tmp/ralfred_clip.txt";
    std::fs::write(p, text)?;
    Command::new("osascript")
        .arg("-e")
        .arg(format!("set the clipboard to (read (POSIX file \"{p}\") as «class utfl»)"))
        .status()?;
    Ok(())
}
