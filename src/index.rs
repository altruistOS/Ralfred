//! Self-built file index for document search — Alfred's model: an index of
//! user-designated folders, kept warm in memory, queried per keystroke in
//! microseconds. Never spawn Spotlight per keystroke (that was the typing
//! lag: mdfind full-disk content queries blocking the UI thread for seconds).

use crate::config::Config;
use crate::search::{Candidate, Kind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Once, OnceLock, RwLock};

pub struct Entry {
    /// lowercased file name
    pub name: String,
    pub path: String,
    pub mtime: i64,
    pub dir: bool,
}

type Idx = Arc<Vec<Entry>>;

static INDEX: OnceLock<RwLock<Idx>> = OnceLock::new();

fn idx() -> &'static RwLock<Idx> {
    INDEX.get_or_init(|| {
        RwLock::new(Arc::new(Vec::new()))
    })
}

static STARTED: AtomicBool = AtomicBool::new(false);
static BUILT: AtomicBool = AtomicBool::new(false);
static LAST_MDFIND: AtomicI64 = AtomicI64::new(0);

static START_ONCE: Once = Once::new();

/// user-designated search folders (config.file_roots; defaults per Alfred's
/// fresh-install scope: Documents / Downloads / Desktop)
pub fn roots() -> Vec<PathBuf> {
    let cfg = Config::load();
    let v: Vec<PathBuf> = cfg
        .file_roots
        .iter()
        .map(|s| crate::config::expand_home(s))
        .filter(|p| p.is_dir())
        .collect();
    if v.is_empty() {
        // config unset or all missing: fall back to the defaults
        return vec![
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Documents"),
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads"),
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Desktop"),
        ];
    }
    v
}

fn skip_dir(name: &str) -> bool {
    name.starts_with('.')
        || name == "node_modules"
        || name == "Library"
        || name.ends_with(".app")
        || name.ends_with(".framework")
        || name.ends_with(".bundle")
        || name.ends_with(".photoslibrary")
        || name.ends_with(".improject")
        || name.ends_with(".keychain")
}

fn walk(root: &Path, out: &mut Vec<Entry>, depth: u8) {
    if depth > 8 || out.len() >= 300_000 {
        return;
    }
    let rd = match std::fs::read_dir(root) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        if out.len() >= 300_000 {
            return;
        }
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let is_dir = p.is_dir();
        if is_dir && skip_dir(&name) {
            continue;
        }
        let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
        out.push(Entry {
            name: name.to_lowercase(),
            path: p.to_string_lossy().to_string(),
            mtime,
            dir: is_dir,
        });
        if is_dir {
            walk(&p, out, depth + 1);
        }
    }
}

fn build() {
    let mut v: Vec<Entry> = Vec::with_capacity(8192);
    for r in roots() {
        walk(&r, &mut v, 0);
    }
    *idx().write().unwrap() = Arc::new(v);
    BUILT.store(true, Ordering::Relaxed);
}

static BUILDING: AtomicBool = AtomicBool::new(false);

/// Like `build()` but gives up if the walk blocks (wedged directory /
/// dead network mount — readdir() has no timeout; the abandoned worker is
/// a leaked OS thread, acceptable for a daemon whose liveness matters).
fn build_guarded(budget: std::time::Duration) {
    if BUILDING.swap(true, Ordering::Relaxed) {
        return; // another attempt is already walking
    }
    let done = Arc::new(AtomicBool::new(false));
    {
        let d = done.clone();
        std::thread::spawn(move || {
            build();
            d.store(true, Ordering::Relaxed);
        });
    }
    let t0 = std::time::Instant::now();
    while !done.load(Ordering::Relaxed) && t0.elapsed() < budget {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    BUILDING.store(false, Ordering::Relaxed);
}

/// Start the background refresh loop once (cheap if already running).
pub fn ensure_started() {
    START_ONCE.call_once(|| {
        STARTED.store(true, Ordering::Relaxed);
        std::thread::spawn(|| {
            loop {
                build_guarded(std::time::Duration::from_secs(10));
                std::thread::sleep(std::time::Duration::from_secs(45));
            }
        });
    });
}

/// Score of name vs query; 0 = no match.
fn score(e: &Entry, q: &str) -> u32 {
    let n = &e.name;
    let mut s = 0u32;
    if n.starts_with(q) {
        s += 1000;
        if n.len() == q.len() {
            s += 250; // exact name
        }
    } else if n.contains(q) {
        // word-boundary start (_q -q .q /space q)
        let idx = n.find(q).unwrap_or(usize::MAX);
        let boundary = idx > 0
            && matches!(n.as_bytes()[idx - 1], b'_' | b'-' | b'.' | b' ' | b'(');
        if boundary {
            s += 600;
        } else {
            s += 300;
        }
        s += (n.len() - q.len()).min(100) as u32 / 8; // shorter names first
    } else {
        return 0;
    }
    s
}

pub fn docs_candidates(q: &str, max: usize) -> Vec<Candidate> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    // first use in this process: build synchronously so CLI queries work;
    // GUI builds in the background via ensure_started() before typing
    if !BUILT.load(Ordering::Relaxed) {
        build_guarded(std::time::Duration::from_millis(1500));
    }
    let arc = idx().read().unwrap().clone();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut hits: Vec<(u32, i64, &Entry)> = Vec::new();
    for e in arc.iter() {
        let s = score(e, &q);
        if s > 0 {
            hits.push((s, e.mtime, e));
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    hits.into_iter()
        .take(max)
        .map(|(s, m, e)| Candidate {
            title: Path::new(&e.path)
                .file_name()
                .map(|x| x.to_string_lossy().to_string())
                .unwrap_or_else(|| e.name.clone()),
            subtitle: e.path.clone(),
            path: Some(e.path.clone()),
            score: 400.0 + (s.min(1900)) as f64 + (m.min(now).max(0) as f64) / 1e9,
            kind: if e.dir { Kind::Dir } else { Kind::File },
            action: None,
        })
        .collect()
}

/// Content-matching fallback through Spotlight — heavily rate-limited (one
/// attempt per 5 s) so it can never storm the disk while typing.
pub fn mdfind_fallback(q: &str, max: usize) -> Vec<Candidate> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let last = LAST_MDFIND.load(Ordering::Relaxed);
    if now - last < 5 {
        return Vec::new();
    }
    LAST_MDFIND.store(now, Ordering::Relaxed);
    let mut out = Vec::new();
    if let Ok(o) = std::process::Command::new("mdfind")
        .arg(format!(
            "kMDItemDisplayName == '*{q}*'c || kMDItemTextContent == '*{q}*'c"
        ))
        .output()
    {
        if o.status.success() {
            for line in String::from_utf8_lossy(&o.stdout).lines().take(max) {
                let p = std::path::Path::new(line);
                if p.to_string_lossy().contains('/') == false {
                    continue;
                }
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| line.to_string());
                let kind = if p.is_dir() { Kind::Dir } else { Kind::File };
                out.push(Candidate {
                    title: name,
                    subtitle: line.to_string(),
                    path: Some(line.to_string()),
                    score: 350.0,
                    kind,
                    action: None,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_search_finds_names_and_skips_bundles() {
        let home = std::env::temp_dir().join(format!("rf-idx-{}", std::process::id()));
        let docs = home.join("Documents");
        std::fs::create_dir_all(docs.join("sub")).unwrap();
        std::fs::write(docs.join("TMRC_2026.pdf"), b"x").unwrap();
        std::fs::write(docs.join("notes.txt"), b"y").unwrap();
        std::fs::create_dir_all(docs.join("Thing.app/Contents")).unwrap();
        std::fs::write(docs.join("Thing.app/Contents/hidden.pdf"), b"z").unwrap();
        std::fs::create_dir_all(docs.join("node_modules")).unwrap();
        std::fs::write(docs.join("node_modules/junk.js"), b"z").unwrap();

        let _lock = crate::tests::env_lock();
        std::env::set_var("RALFRED_HOME", &home);
        // override roots via config (full valid JSON: Config has no serde
        // defaults for its required fields — a partial file would parse-fail
        // and silently fall back to the real ~/Documents)
        let gdir = home.join("Alfred.alfredpreferences/preferences/global");
        std::fs::create_dir_all(&gdir).unwrap();
        std::fs::write(
            gdir.join("ralfred_config.json"),
            format!(
                r#"{{"default_scope":["/Applications"],"max_results":20,
                 "auto_expand_snippets":true,"clipboard_max_items":10,
                 "web_search":{{}},"currency":"USD","default_browser":"",
                 "theme":"alfred-dark","hotkey":"ALT+CTRL+SPACE",
                 "translucent":true,"features_enabled":{{}},
                 "file_roots":["{}"]}}"#,
                docs.display()
            ),
        )
        .unwrap();

        BUILT.store(false, Ordering::Relaxed);
        let hits = docs_candidates("TMRC", 10);
        assert!(hits.iter().any(|h| h.title.starts_with("TMRC")), "{:?}", hits);
        // .app internals and node_modules must not be indexed
        let junk = docs_candidates("hidden", 10);
        assert!(!junk.iter().any(|h| h.subtitle.contains("hidden.pdf")));
        let j2 = docs_candidates("junk", 10);
        assert!(!j2.iter().any(|h| h.subtitle.contains("junk.js")));

        std::env::remove_var("RALFRED_HOME");
        std::fs::remove_dir_all(&home).ok();
    }
}
