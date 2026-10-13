//! Configuration — mirrors Alfred's prefs tree at
//! `~/Library/Application Support/Alfred/Alfred.alfredpreferences/preferences/`.
//! We persist our equivalent as plist (global) + JSON (session overrides).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        }
    }
}

fn global_config_path() -> PathBuf {
    prefs_dir().join("global/ralfred_config.json")
}

impl Config {
    pub fn load() -> Self {
        let path = global_config_path();
        if path.exists() {
            if let Ok(s) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<Config>(&s) {
                    return cfg;
                }
            }
        }
        let cfg = Self::default();
        let _ = cfg.save();
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = global_config_path();
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, serde_json::to_string_pretty(self).unwrap())?;
        Ok(())
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
