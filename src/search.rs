//! Default results — application & document discovery with usage ranking,
//! equivalent to AlfredFeatureDefaultSearch + knowledge.alfdb (no Spotlight dep;
//! uses walkdir scope scan + mdfind fallback).

use crate::config::{expand_home, Config};
use crate::db::Store;
use walkdir::WalkDir;

#[derive(Debug, Clone, Default)]
pub struct Candidate {
    pub title: String,
    pub subtitle: String,
    pub path: Option<String>,
    pub score: f64,
    pub kind: Kind,
    /// Explicit action from a workflow graph node (openfile/openurl/terminal/copy).
    pub action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Kind {
    App,
    PrefPane,
    Dir,
    File,
    WebSearch,
    Snippet,
    Clipboard,
    Calc,
    Dictionary,
    #[default]
    Action,
}

impl Kind {
    pub fn label(&self) -> &'static str {
        match self {
            Kind::App => "application",
            Kind::PrefPane => "preference pane",
            Kind::Dir => "folder",
            Kind::File => "document",
            Kind::WebSearch => "web search",
            Kind::Snippet => "snippet",
            Kind::Clipboard => "clipboard",
        Kind::Dictionary => "dictionary",
            Kind::Calc => "calculation",
            Kind::Action => "action",
        }
    }
}

pub fn apps_in(scope: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dir in scope {
        let root = expand_home(dir);
        if !root.exists() {
            continue;
        }
        for e in WalkDir::new(root)
            .min_depth(1)
            .max_depth(3)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| {
                let p = e.path().to_string_lossy();
                !p.contains("/Contents/") && !p.contains(".framework") && !p.contains("/Xcode.app/Contents/Applications/")
            })
        {
            if let Ok(d) = e {
                let p = d.path();
                if d.file_type().is_dir() && p.extension().map(|x| x == "app").unwrap_or(false) {
                    if let Some(name) = p.file_stem().map(|s| s.to_string_lossy().to_string()) {
                        out.push((name, p.to_string_lossy().to_string()));
                    }
                } else if p.extension().map(|x| x == "prefPane").unwrap_or(false) {
                    if let Some(name) = p.file_stem().map(|s| s.to_string_lossy().to_string()) {
                        out.push((name, p.to_string_lossy().to_string()));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.dedup_by(|a, b| a.1 == b.1);
    out
}

/// Fuzzy scoring — Alfred-like: exact prefix >> word-start >> subsequence.
pub fn fuzzy_score(query: &str, target: &str) -> f64 {
    let q = query.to_lowercase();
    let t = target.to_lowercase();
    if t.is_empty() {
        return 0.0;
    }
    if t == q {
        return 1000.0;
    }
    let mut score = 0.0;
    if t.starts_with(&q) {
        score += 500.0;
    } else if t.replace(' ', "").starts_with(&q.replace(' ', "")) {
        score += 200.0;
    } else {
        // word starts
        for w in t.split([' ', '-', '_', '/']) {
            if w.starts_with(&q) {
                score += 150.0;
            }
        }
    }
    // subsequence match anywhere. Compare chars, not bytes: indexing the
    // string by byte would mis-score (and could split) non-ASCII names.
    let tchars: Vec<char> = t.chars().collect();
    let mut ti = 0usize;
    let mut hits = 0usize;
    let mut last_pos = -1i64;
    for qc in q.chars() {
        while ti < tchars.len() && tchars[ti] != qc {
            ti += 1;
        }
        if ti < tchars.len() {
            hits += 1;
            if ti as i64 - last_pos > 1 {
                score += 5.0; // gaps are okay but penalised later
            }
            last_pos = ti as i64;
            ti += 1;
        }
    }
    if hits < q.chars().count() {
        return 0.0; // not even a subsequence
    }
    score += 10.0 * hits as f64 / (1.0 + q.len() as f64);
    score + (100.0 / (t.len() as f64 + 1.0)) // shorter names rank higher
}

/// Default-results: apps + dirs + knowledge ranking.
pub fn default_results(query: &str, cfg: &Config, store: &Store, max: usize) -> Vec<Candidate> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let now = crate::db::now_ts();
    let usage: std::collections::HashMap<String, f64> = store
        .last_used("")
        .into_iter()
        .map(|(k, ts)| (k, ts))
        .collect();
    let mut out: Vec<Candidate> = Vec::new();
    for (name, path) in apps_in(&cfg.default_scope) {
        let mut s = fuzzy_score(query, &name);
        if s > 0.0 {
            if let Some(ts) = usage.get(&path) {
                let age_days = (now - ts) / 86400.0;
                s += 300.0 / (1.0 + age_days * 0.5);
            }
            out.push(Candidate {
                title: name,
                subtitle: path.clone(),
                path: Some(path),
                score: s,
                kind: Kind::App,
                action: None,
            });
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out.truncate(max);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_score_handles_non_ascii() {
        // Regression: the subsequence scan used to index the target by byte and
        // compare against chars, which mis-scored and could split a multi-byte
        // character (SPEC §5.2 name matching).
        assert!(fuzzy_score("caf", "Café Déjà") > 0.0);
        assert!(fuzzy_score("é", "Café") > 0.0);
        assert_eq!(fuzzy_score("zzz", "Café"), 0.0);
        // A query that is only a byte-prefix of a multi-byte char must not panic.
        assert_eq!(fuzzy_score("ca", "é"), 0.0);
        let _ = fuzzy_score("x", "日本語");
    }

    #[test]
    fn fuzzy_ranks_exact_prefix_first() {
        assert!(fuzzy_score("saf", "Safari") > fuzzy_score("saf", "Xcode Safety"));
    }

    #[test]
    fn fuzzy_empty_target_scores_zero() {
        assert_eq!(fuzzy_score("x", ""), 0.0);
    }
}
