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

/// Score band for an explicit keyword hit (snippet, workflow, web search,
/// dictionary). These are deliberate keyword dispatches, so they outrank the
/// default-result band (SPEC §3.2 orders keyword features before default
/// results).
const KEYWORD_HIT: f64 = 3000.0;

/// Lift the application/preference-pane band above the file-index band so that
/// a query naming an app ranks the app first (SPEC §3.2, §5.2). Tuned so an
/// app prefix match still beats an exactly-named document, while an app match
/// that is only a loose subsequence does not.
const DEFAULT_RESULT_BONUS: f64 = 1200.0;

/// Alfred's File Search pane exposes a Quick File Search prefix (`'`, and the
/// spacebar on an empty query) plus four file keywords: `open`, `find`, `in`
/// and `tags`. Each selects a dedicated file result set (SPEC §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileSearchMode {
    Quick,
    Open,
    Reveal,
    Content,
    Tags,
}

/// Split a query into a file-search mode and its search term. `None` means the
/// query does not begin with one of those prefixes or keywords. The keywords
/// require a following space so that the bare words `open`/`find`/`in`/`tags`
/// still behave as ordinary queries.
fn file_search_mode(q: &str) -> Option<(FileSearchMode, &str)> {
    if let Some(rest) = q.strip_prefix('\'') {
        return Some((FileSearchMode::Quick, rest.trim()));
    }
    for (kw, mode) in [
        ("open", FileSearchMode::Open),
        ("find", FileSearchMode::Reveal),
        ("in", FileSearchMode::Content),
        ("tags", FileSearchMode::Tags),
    ] {
        if let Some(rest) = q.strip_prefix(kw).and_then(|r| r.strip_prefix(' ')) {
            return Some((mode, rest.trim()));
        }
    }
    None
}

pub enum Action {    Open(PathBuf),
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
                    score: KEYWORD_HIT,
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
    // Dispatch on *any* of a workflow's keywords, not just its first: real
    // workflows carry several keyword inputs (e.g. "2show" and "show") and
    // registering only the first makes the rest permanently unreachable.
    let mut matching: Vec<(&crate::workflow::Workflow, String)> = Vec::new();
    for wf in &wfs {
        for kw in &wf.keywords {
            if *kw == maybe_kw || q.starts_with(kw.as_str()) {
                matching.push((wf, kw.clone()));
                break; // one dispatch per workflow
            }
        }
    }
    // SPEC §8.1 — a token claimed by more than one feature must not silently run
    // one of them. Distinct workflows count separately; two on-disk copies of the
    // same workflow (same bundle id) are one claimant, not a conflict.
    let mut wf_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (wf, _) in &matching {
        wf_ids.insert(if wf.bundleid.is_empty() {
            wf.uid.as_str()
        } else {
            wf.bundleid.as_str()
        });
    }
    let web_claims = cfg.web_search.contains_key(&maybe_kw);
    let snippet_claims = crate::features::enabled("snippets")
        && Store::snippets()
            .find_snippets(&maybe_kw)
            .iter()
            .any(|s| s.keyword == maybe_kw);
    if wf_ids.len() + usize::from(web_claims) + usize::from(snippet_claims) > 1 {
        let mut who: Vec<String> = matching
            .iter()
            .map(|(w, _)| format!("workflow '{}'", w.name))
            .collect();
        who.dedup();
        if web_claims {
            who.push(format!("web search '{maybe_kw}'"));
        }
        if snippet_claims {
            who.push(format!("snippet '{maybe_kw}'"));
        }
        return vec![Candidate {
            title: format!("'{maybe_kw}' cannot run — keyword conflict"),
            subtitle: format!("claimed by {}", who.join(" and ")),
            path: None,
            score: KEYWORD_HIT + 1.0,
            kind: Kind::Action,
            action: None,
        }];
    }

    for (wf, kw) in &matching {
        let rest = if q.starts_with(kw.as_str()) {
            q[kw.len()..].trim_start()
        } else {
            ""
        };
        let mut items = wf.run_keyword_for(kw, rest).0;
        for it in &mut items {
            if it.subtitle.is_empty() {
                it.subtitle = format!("[{}]", wf.name);
            } else {
                it.subtitle = format!("{} · {}", it.subtitle, wf.name);
            }
            it.score = KEYWORD_HIT;
        }
        results.append(&mut items);
    }

    // 4. web search keyword
    for (k, tmpl) in &cfg.web_search {
        if q == *k || q.starts_with(&format!("{k} ")) {
            let query = if q == *k { "" } else { q[k.len() + 1..].trim() };
            let url = tmpl
                .replace("{query}", &url_encode(query))
                .replace("{q}", &url_encode(query));
            let url = if url.contains("://") || url.starts_with("magnet:") {
                url
            } else {
                format!("https://{url}")
            };
            results.push(Candidate {
                title: format!("Search {} for '{}'", k, query),
                subtitle: url.clone(),
                path: Some(url),
                score: KEYWORD_HIT,
                kind: Kind::WebSearch,
                action: None,
            });
        }
    }

    // 4b. File-search keywords and Quick File Search (observed in Alfred's
    // File Search preferences pane: the Quick Search prefix `'` or spacebar,
    // and the keywords `open`, `find`, `in` and `tags`). These are deliberate
    // file searches, so they take precedence over default results and return a
    // dedicated result set.
    if crate::features::enabled("filesearch") {
        if let Some((mode, term)) = file_search_mode(q) {
            if !term.is_empty() {
                match mode {
                    FileSearchMode::Tags => {
                        // The scoped index carries names only; tag metadata is
                        // not collected, so say so instead of returning nothing.
                        return vec![Candidate {
                            title: format!("No tag index for '{term}' — ralfred's file index stores names only"),
                            subtitle: "tags require Spotlight metadata; use 'in' for content".into(),
                            path: None,
                            score: KEYWORD_HIT,
                            kind: Kind::Action,
                            action: None,
                        }];
                    }
                    FileSearchMode::Content => {
                        if std::env::var("RALFRED_CONTENT_SEARCH").is_ok() {
                            let mut hits = crate::index::mdfind_fallback(term, 40);
                            for h in &mut hits {
                                h.score += DEFAULT_RESULT_BONUS;
                            }
                            if !hits.is_empty() {
                                return hits;
                            }
                        } else {
                            return vec![Candidate {
                                title: format!("Content search is off — set RALFRED_CONTENT_SEARCH=1 to search inside files for '{term}'"),
                                subtitle: "Spotlight content queries are opt-in (they are expensive per keystroke)".into(),
                                path: None,
                                score: KEYWORD_HIT,
                                kind: Kind::Action,
                                action: None,
                            }];
                        }
                    }
                    FileSearchMode::Quick | FileSearchMode::Open | FileSearchMode::Reveal => {
                        let mut hits = crate::index::docs_candidates(term, 40);
                        for h in &mut hits {
                            h.score += DEFAULT_RESULT_BONUS;
                            if let Some(p) = h.path.clone() {
                                match mode {
                                    FileSearchMode::Open => h.action = Some(format!("openfile:{p}")),
                                    FileSearchMode::Reveal => h.action = Some(format!("revealfile:{p}")),
                                    _ => {}
                                }
                            }
                        }
                        if !hits.is_empty() {
                            return hits;
                        }
                    }
                }
            }
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
                score: KEYWORD_HIT,
                kind: Kind::Dictionary,
                action: None,
            });
        }
    }

    // 5. default results (apps / pref panes)
    if crate::features::enabled("filesearch") {
        let mut dr = default_results(q, cfg, store, cfg.max_results);
        for r in &mut dr {
            // Applications and preference panes are Alfred's *default* results;
            // file/document matches are a secondary section (SPEC §3.2). The
            // index scores a strong filename match up to ~1650, so the default
            // band is lifted above it: a query naming an app surfaces the app
            // first, while a weak app match still loses to an exact filename.
            r.score += DEFAULT_RESULT_BONUS;
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

    // `total_cmp` keeps a NaN score from panicking and still orders
    // 7b. path completion — a path-prefixed query enters file mode and offers
    // live parent-directory completion (SPEC §5.2). This band belongs to the
    // router so that it runs *before* the fallback band; otherwise a
    // single-token path query would be answered by web fallbacks and never
    // reach path completion.
    if results.is_empty() && (q.starts_with('/') || q.starts_with("~/")) {
        results.extend(path_candidates(q));
    }

    // 8. fallbacks — offered only when nothing else matched (Default Results
    // pane: "Only show fallbacks when there are no results"; the pane notes
    // "By default, Alfred shows Google, Amazon and Wikipedia"). Deliberate
    // narrowing: only a single-token query falls back, so a mistyped multi-word
    // feature keyword does not silently turn into a web search (SPEC §5.1).
    if results.is_empty()
        && cfg.fallbacks
        && q.chars().count() >= 2
        && !q.chars().any(char::is_whitespace)
        // A query that named a file-search mode asked for files; answering it
        // with web fallbacks (and echoing the mode prefix into the query) would
        // be wrong, so those modes never fall back (SPEC §8.21).
        && file_search_mode(q).is_none()
    {
        for (name, url) in [
            ("Google", "https://www.google.com/search?q={query}"),
            ("Amazon", "https://www.amazon.com/s?k={query}"),
            (
                "Wikipedia",
                "https://en.wikipedia.org/wiki/Special:Search?search={query}",
            ),
        ] {
            let target = url.replace("{query}", &url_encode(q));
            results.push(Candidate {
                title: format!("Search {name} for '{q}'"),
                subtitle: format!("Fallback — {target}"),
                path: Some(target),
                score: 10.0,
                kind: Kind::WebSearch,
                action: None,
            });
        }
    }

    // deterministically (descending).
    results.sort_by(|a, b| b.score.total_cmp(&a.score));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// SPEC §3.2 orders keyword features before default results, and default
    /// results (applications) before file/document matches. These are the band
    /// relationships that ordering depends on; they are asserted here so a
    /// future score tweak cannot silently invert the list.
    #[test]
    fn result_bands_are_ordered_keyword_apps_documents() {
        let app_exact = 1000.0 + DEFAULT_RESULT_BONUS; // exact application name
        let app_prefix = 500.0 + DEFAULT_RESULT_BONUS; // application name prefix
        let doc_exact = 400.0 + 1250.0; // exact filename, max index score
        assert!(
            KEYWORD_HIT > app_exact,
            "an explicit keyword hit must outrank an application"
        );
        assert!(
            app_exact > doc_exact,
            "an application must outrank an exactly-named document"
        );
        assert!(
            app_prefix > doc_exact,
            "an application prefix match must still beat an exactly-named document"
        );
    }

    /// SPEC §8.1 — a token claimed by a workflow *and* by a web-search keyword
    /// must be reported, not silently dispatched to one of them.
    #[test]
    fn keyword_conflict_is_reported_not_dispatched() {
        let _guard = crate::tests::env_lock();
        let home = std::env::temp_dir().join(format!("ralfred_router_c_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("RALFRED_HOME", &home);
        let pack = crate::config::workflows_dir().join("Conflict.wfunpack");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(
            pack.join("info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>bundleid</key><string>com.example.conflict</string>
<key>name</key><string>Conflict</string>
<key>connections</key><dict/>
<key>objects</key><array>
  <dict><key>uid</key><string>SF</string>
  <key>type</key><string>alfred.workflow.input.scriptfilter</string>
  <key>config</key><dict><key>keyword</key><string>g</string>
  <key>type</key><integer>2</integer>
  <key>script</key><string>echo '{"items":[{"title":"WFHIT"}]}'</string></dict></dict>
</array>
</dict></plist>"#,
        )
        .unwrap();
        let cfg = Config::load();
        let store = Store::knowledge();
        let res = route("g rust", &cfg, &store);
        std::env::remove_var("RALFRED_HOME");
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(res.len(), 1, "only the conflict row is returned: {res:?}");
        assert!(res[0].title.contains("conflict"), "{:?}", res[0].title);
        assert!(res[0].path.is_none(), "a conflict row must not be actionable");
        assert!(
            !res.iter().any(|c| c.title == "WFHIT"),
            "the colliding workflow must not run"
        );
    }

    /// SPEC §8.6 — an empty or whitespace-only query yields nothing.
    #[test]
    fn blank_query_returns_no_results() {
        let _guard = crate::tests::env_lock();
        let home = std::env::temp_dir().join(format!("ralfred_router_b_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("RALFRED_HOME", &home);
        let cfg = Config::load();
        let store = Store::knowledge();
        let blank = route("   ", &cfg, &store);
        let empty = route("", &cfg, &store);
        std::env::remove_var("RALFRED_HOME");
        let _ = std::fs::remove_dir_all(&home);
        assert!(blank.is_empty());
        assert!(empty.is_empty());
    }

    /// SPEC §5.2 — the File Search pane's Quick Search prefix and its four
    /// keywords parse into distinct modes, and a keyword only applies when it
    /// is a whole word (followed by a space).
    #[test]
    fn file_search_mode_parses_alfred_prefixes_and_keywords() {
        assert_eq!(file_search_mode("'html"), Some((FileSearchMode::Quick, "html")));
        assert_eq!(file_search_mode("open report"), Some((FileSearchMode::Open, "report")));
        assert_eq!(file_search_mode("find report"), Some((FileSearchMode::Reveal, "report")));
        assert_eq!(file_search_mode("in report"), Some((FileSearchMode::Content, "report")));
        assert_eq!(file_search_mode("tags report"), Some((FileSearchMode::Tags, "report")));
        // A keyword needs its following space, so these stay ordinary queries.
        for q in ["open", "find", "in", "tags", "op github", "openx y", "index"] {
            assert_eq!(file_search_mode(q), None, "{q} must not enter file mode");
        }
    }

    /// SPEC §5.2 — `tags` has no tag metadata in the scoped index, and `in`
    /// needs the opt-in content path. Both must explain themselves rather than
    /// return a silent empty list.
    #[test]
    fn file_search_keywords_report_unavailable_backends() {
        let _guard = crate::tests::env_lock();
        let home = std::env::temp_dir().join(format!("ralfred_router_fs_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("RALFRED_HOME", &home);
        std::env::remove_var("RALFRED_CONTENT_SEARCH");
        let cfg = Config::load();
        let store = Store::knowledge();

        let tags = route("tags rust", &cfg, &store);
        let inside = route("in rust", &cfg, &store);
        std::env::remove_var("RALFRED_HOME");
        let _ = std::fs::remove_dir_all(&home);

        assert_eq!(tags.len(), 1, "tags must yield one explanatory row: {tags:?}");
        assert!(tags[0].title.contains("tag index"), "{:?}", tags[0].title);
        assert!(tags[0].path.is_none(), "an unavailable backend is not actionable");
        assert_eq!(inside.len(), 1, "in must yield one explanatory row: {inside:?}");
        assert!(
            inside[0].title.contains("Content search is off"),
            "{:?}",
            inside[0].title
        );
    }

    /// SPEC §3.2 step 5 / §5.1 — fallbacks appear only when nothing else
    /// matched, and only for a single-token query.
    #[test]
    fn fallbacks_only_when_nothing_matched_and_single_token() {
        let _guard = crate::tests::env_lock();
        let home = std::env::temp_dir().join(format!("ralfred_router_fb_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("RALFRED_HOME", &home);
        let cfg = Config::load();
        let store = Store::knowledge();

        let miss = route("zzqxjv", &cfg, &store);
        let multi = route("zzqxjv wwqk", &cfg, &store);
        let mut off = Config::load();
        off.fallbacks = false;
        let disabled = route("zzqxjv", &off, &store);
        std::env::remove_var("RALFRED_HOME");
        let _ = std::fs::remove_dir_all(&home);

        assert_eq!(miss.len(), 3, "Google/Amazon/Wikipedia are the default fallbacks: {miss:?}");
        assert!(miss[0].title.contains("Google"), "{:?}", miss[0].title);
        assert!(miss.iter().any(|c| c.title.contains("Amazon")));
        assert!(miss.iter().any(|c| c.title.contains("Wikipedia")));
        assert!(miss[0].path.as_deref().unwrap_or("").contains("zzqxjv"));
        assert!(
            multi.is_empty(),
            "a mistyped multi-word keyword must not become a web search: {multi:?}"
        );
        assert!(disabled.is_empty(), "fallbacks:false suppresses the band");
        // A query that named a file-search mode asked for files, so it must not
        // be answered with web fallbacks echoing the prefix (SPEC §8.21).
        let quick = route("'zzqxjv", &cfg, &store);
        assert!(
            quick.is_empty(),
            "quick file search must not fall back to the web: {quick:?}"
        );
    }
}
