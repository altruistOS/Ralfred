//! Configuration — mirrors Alfred's prefs tree at
//! `~/Library/Application Support/Alfred/Alfred.alfredpreferences/preferences/`.
//! We persist our equivalent as plist (global) + JSON (session overrides).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn app_support_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("RALFRED_HOME") {
        return PathBuf::from(custom);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/Application Support/ralfred")
}

pub fn prefs_dir() -> PathBuf {
    app_support_dir().join("Alfred.alfredpreferences/preferences")
}

pub fn data_dir() -> PathBuf {
    app_support_dir().join("Databases")
}

pub fn workflows_dir() -> PathBuf {
    prefs_dir().join("workflows")
}

/// Configuration — mirrors Alfred's prefs tree at
/// `~/Library/Application Support/Alfred/Alfred.alfredpreferences/preferences/`.
/// We persist our equivalent as plist (global) + JSON (session overrides).
///
/// A partial configuration file is legal: every key missing from the JSON is
/// filled in from `Default` (SPEC §8.23). Without the container-level
/// `#[serde(default)]`, a hand-edited file missing a single required key
/// failed to parse and `load()` silently returned the built-in defaults —
/// the user's settings appeared to reset.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Directory names to index for default (app/doc) results.
    pub default_scope: Vec<String>,
    /// Max results shown.
    pub max_results: usize,
    /// Snippet auto-expand on typing (snippets.db).
    pub auto_expand_snippets: bool,
    /// Clipboard history rows.
    pub clipboard_max_items: usize,
    /// Web search keyword map (e.g. g, yt).
    pub web_search: std::collections::HashMap<String, String>,
    /// Calculator locale/currency.
    pub currency: String,
    /// Default browser bundle id for opening URLs.
    pub default_browser: String,
    /// Theme (reserved for later; colors kept out of clean-room).
    pub theme: String,
    /// Global hotkey to summon the panel (Alfred's equivalent is a user choice).
    pub hotkey: String,
    /// macOS vibrancy (NSVisualEffectView) behind the panel — the frosted-glass
    /// look Alfred ships as "translucency". Default on, like Alfred.
    #[serde(default = "default_true")]
    pub translucent: bool,
    /// Per-feature-domain enable flags (Features pane). Absent key = enabled.
    #[serde(default)]
    pub features_enabled: std::collections::HashMap<String, bool>,
    /// Launch behavior: "panel" summons the search box, "silent" leaves the
    /// app in the menu bar (Alfred's default is menu-bar-only).
    #[serde(default = "default_launch_mode")]
    pub launch_mode: String,
    /// Folders the file index walks (Alfred's "search in these locations").
    /// Empty = built-in defaults (Documents / Downloads / Desktop).
    #[serde(default = "default_file_roots")]
    pub file_roots: Vec<String>,
    /// Fallbacks band (Default Results pane: "Only show fallbacks when there
    /// are no results"). When true and no other band produced a result, the
    /// router offers web searches for the typed text (SPEC §3.2 step 5).
    #[serde(default = "default_true")]
    pub fallbacks: bool,
}

impl Default for Config {
    fn default() -> Self {
        let mut web = std::collections::HashMap::new();
        for (k, u) in [
            ("g", "https://www.google.com/search?q={query}"),
            ("yt", "https://www.youtube.com/results?search_query={query}"),
            ("wiki", "https://en.wikipedia.org/wiki/{query}"),
        ] {
            web.insert(k.into(), u.into());
        }
        Self {
            default_scope: vec![
                "/Applications".into(),
                "/System/Applications".into(),
                "/System/Library/PreferencePanes".into(),
                "~/Applications".into(),
                "~/Library/PreferencePanes".into(),
            ],
            max_results: 20,
            auto_expand_snippets: true,
            clipboard_max_items: 500,
            web_search: web,
            currency: "USD".into(),
            default_browser: String::new(),
            theme: "alfred-dark".into(),
            hotkey: "ALT+CTRL+SPACE".into(),
            translucent: true,
            file_roots: Vec::new(),
            fallbacks: true,
            launch_mode: "panel".into(),
            features_enabled: std::collections::HashMap::new(),
        }
    }
}

fn global_config_path() -> PathBuf {
    prefs_dir().join("global/ralfred_config.json")
}

impl Config {
    /// Read the configuration. A missing or unreadable file yields the built-in
    /// defaults; reading never writes. (It used to persist the defaults as a
    /// side effect, so any concurrent reader could overwrite a configuration
    /// another writer had just saved — see SPEC §8.13.)
    pub fn load() -> Self {
        let path = global_config_path();
        if path.exists() {
            if let Ok(s) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<Config>(&s) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = global_config_path();
        let dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir)?;
        let body = serde_json::to_string_pretty(self).unwrap();
        // Atomic replace (SPEC §8.13): write a sibling temp file and rename it
        // over the target, so a concurrent reader never observes a half-written
        // config and two writers cannot interleave their bytes.
        let tmp = dir.join(format!(".config.json.tmp.{}", std::process::id()));
        std::fs::write(&tmp, body)?;
        match std::fs::rename(&tmp, &path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }
}

pub fn expand_home(p: &str) -> PathBuf {
    let p = if let Some(rest) = p.strip_prefix("~/") {
        dirs::home_dir().unwrap_or_default().join(rest)
    } else if p == "~" {
        dirs::home_dir().unwrap_or_default()
    } else {
        PathBuf::from(p)
    };
    p
}

pub fn default_true() -> bool {
    true
}
fn default_file_roots() -> Vec<String> { Vec::new() }
fn default_launch_mode() -> String { "panel".into() }

#[cfg(test)]
mod tests {
    use super::*;

    /// SPEC §8.13 — configuration writes are atomic (write-temp-then-rename):
    /// the value round-trips and no temporary file is left behind.
    #[test]
    fn save_is_atomic_and_round_trips() {
        let _guard = crate::tests::env_lock();
        let home = std::env::temp_dir().join(format!("ralfred_cfg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("RALFRED_HOME", &home);
        let mut cfg = Config::default();
        cfg.max_results = 17;
        cfg.save().expect("save");
        let dir = global_config_path().parent().unwrap().to_path_buf();
        let litter: Vec<String> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|n| n.starts_with(".config.json.tmp"))
            .collect();
        let back = Config::load();
        std::env::remove_var("RALFRED_HOME");
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(back.max_results, 17, "the saved value must round-trip");
        assert!(litter.is_empty(), "temp files left behind: {litter:?}");
    }

    /// SPEC §8.23 — a configuration file that specifies only some keys must
    /// load: the missing keys take their defaults and the specified keys are
    /// honoured. This is how a hand-edited file behaves; before the fix such a
    /// file failed to parse and every setting silently reverted.
    #[test]
    fn partial_config_file_loads_with_defaults() {
        let _guard = crate::tests::env_lock();
        let home = std::env::temp_dir().join(format!("ralfred_cfgp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("RALFRED_HOME", &home);
        let path = global_config_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"file_roots": ["/tmp/rf_partial"], "fallbacks": false}"#,
        )
        .unwrap();
        let cfg = Config::load();
        std::env::remove_var("RALFRED_HOME");
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(cfg.file_roots, vec!["/tmp/rf_partial".to_string()]);
        assert!(!cfg.fallbacks, "the specified key must be honoured");
        // everything not mentioned comes from Default, not from a parse failure
        assert_eq!(cfg.max_results, 20);
        assert_eq!(cfg.hotkey, "ALT+CTRL+SPACE");
        assert!(cfg.file_roots.iter().all(|r| r == "/tmp/rf_partial"));
        assert_eq!(
            Config::default().max_results,
            20,
            "the default referenced by the fallback must stay in sync"
        );
    }
}
