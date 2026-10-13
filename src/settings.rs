//! Settings backend for the in-app preferences (Alfred keeps a Settings
//! window; ours is an overlay in the same panel, opened with Cmd+, — the
//! macOS convention). All state lives in Config, so the CLI and the GUI see
//! the same values; this module just presents them as a JSON document.

use crate::config::Config;
use serde_json::{json, Value};

pub fn schema() -> Value {
    json!({
        "sections": [
            {"id": "general", "title": "General", "fields": [
                {"key": "hotkey", "type": "string", "label": "Show/hide hotkey",
                 "hint": "ALT+CTRL+SPACE, CMD+OPT+K, …", "restart": true},
                {"key": "launch_at_login", "type": "bool", "label": "Start at login",
                 "hint": "registers the launchd agent", "restart": true},
            ]},
            {"id": "appearance", "title": "Appearance", "fields": [
                {"key": "theme", "type": "select", "label": "Theme",
                 "options": ["alfred-dark", "alfred-light", "midnight"]},
            ]},
            {"id": "features", "title": "Features", "fields": [
                {"key": "auto_expand_snippets", "type": "bool", "label": "Snippet auto expansion"},
                {"key": "max_results", "type": "int", "label": "Max results", "min": 5, "max": 50},
                {"key": "clipboard_max_items", "type": "int", "label": "Clipboard history size", "min": 50, "max": 1000},
            ]},
            {"id": "defaultsearch", "title": "Default Results", "fields": [
                {"key": "default_scope", "type": "list", "label": "Folders to search",
                 "hint": "one path per line, ~ allowed"},
            ]},
            {"id": "websearch", "title": "Web Search", "fields": [
                {"key": "web_search", "type": "map", "label": "Keyword → URL template",
                 "hint": "use {query} in the URL"},
            ]},
        ]
    })
}

pub fn get() -> Value {
    let cfg = Config::load();
    let install_loaded = crate::install::status().starts_with("installed");
    json!({
        "hotkey": cfg.hotkey,
        "launch_at_login": install_loaded,
        "theme": cfg.theme,
        "auto_expand_snippets": cfg.auto_expand_snippets,
        "max_results": cfg.max_results,
        "clipboard_max_items": cfg.clipboard_max_items,
        "default_scope": cfg.default_scope,
        "web_search": cfg.web_search,
        "version": env!("CARGO_PKG_VERSION"),
    })
}

/// PATCH partial settings. Returns Err(reason) on invalid input; valid
/// changes persist to Config (and register/unregister the login item).
pub fn patch(body: &Value) -> Result<(), String> {
    let mut cfg = Config::load();
    if let Some(h) = body.get("hotkey").and_then(|v| v.as_str()) {
        crate::hotkey::parse_combo(h).map_err(|e| format!("bad hotkey: {e}"))?;
        cfg.hotkey = h.to_string();
    }
    if let Some(t) = body.get("theme").and_then(|v| v.as_str()) {
        if !["alfred-dark", "alfred-light", "midnight"].contains(&t) {
            return Err("unknown theme".into());
        }
        cfg.theme = t.to_string();
    }
    if let Some(v) = body.get("auto_expand_snippets").and_then(|v| v.as_bool()) {
        cfg.auto_expand_snippets = v;
    }
    if let Some(v) = body.get("max_results").and_then(|v| v.as_u64()) {
        if !(5..=50).contains(&v) {
            return Err("max_results out of range".into());
        }
        cfg.max_results = v as usize;
    }
    if let Some(v) = body.get("clipboard_max_items").and_then(|v| v.as_u64()) {
        cfg.clipboard_max_items = v.clamp(50, 10000) as usize;
    }
    if let Some(list) = body.get("default_scope").and_then(|v| v.as_array()) {
        let mut v = Vec::new();
        for p in list {
            if let Some(s) = p.as_str() {
                v.push(s.to_string());
            }
        }
        cfg.default_scope = v;
    }
    if let Some(map) = body.get("web_search").and_then(|v| v.as_object()) {
        let mut m = std::collections::HashMap::new();
        for (k, u) in map {
            if let (ks, Some(us)) = (k.as_str(), u.as_str()) {
                if ks.contains(' ') || !us.starts_with("http") {
                    return Err(format!("bad web keyword: {ks}"));
                }
                m.insert(ks.to_string(), us.to_string());
            }
        }
        cfg.web_search = m;
    }
    if let Some(lal) = body.get("launch_at_login").and_then(|v| v.as_bool()) {
        let _ = if lal {
            crate::install::install(&cfg.hotkey.clone())
        } else {
            crate::install::uninstall()
        };
    }
    cfg.save().map_err(|e| format!("save: {e}"))?;
    Ok(())
}
