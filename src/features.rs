//! Feature domains — mirrors Alfred's `AlfredFeature*` functional areas
//! (evidence: docs/rea-evidence/02-binary-evidence.md class inventory).
//! The Preferences "Features" pane lists these; the enabled flag is persisted
//! in Config and honored by the router.

use crate::config::Config;
use serde_json::{json, Value};

pub struct Feature {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub glyph: &'static str,
}

pub const FEATURES: &[Feature] = &[
    Feature { id: "filesearch", name: "File Search", desc: "Find applications, documents and folders by name; open, reveal and act on them.", glyph: "F" },
    Feature { id: "actions", name: "Universal Actions", desc: "Press Tab on a result to run file, folder and app actions.", glyph: "U" },
    Feature { id: "bookmarks", name: "Web Bookmarks", desc: "Search your Safari and Chrome bookmarks from the panel.", glyph: "B" },
    Feature { id: "music", name: "Music", desc: "Control the Music app library, playback and artists.", glyph: "M" },
    Feature { id: "contacts", name: "Contacts", desc: "Look up and open contacts from the macOS address book.", glyph: "C" },
    Feature { id: "onepassword", name: "1Password", desc: "Search, view and open-and-fill 1Password items.", glyph: "1" },
    Feature { id: "clipboard", name: "Clipboard History", desc: "Record clipboard items and recall them with the , keyword.", glyph: "V" },
    Feature { id: "snippets", name: "Snippet Expansions", desc: "Type an abbreviation to expand a saved snippet.", glyph: "S" },
    Feature { id: "workflows", name: "Workflows", desc: "Run `.alfredworkflow` bundles and script filters.", glyph: "W" },
    Feature { id: "calculator", name: "Calculator", desc: "Inline math with units, constants and base conversions.", glyph: "=" },
    Feature { id: "dictionary", name: "Dictionary Lookup", desc: "Look up words against the installed dictionaries.", glyph: "D" },
    Feature { id: "largetype", name: "Large Type", desc: "Magnify a result in full screen with Cmd+L.", glyph: "L" },
    Feature { id: "quicklook", name: "Quick Look", desc: "Preview files without opening them (Space / Cmd+Y).", glyph: "Q" },
    Feature { id: "terminal", name: "Terminal", desc: "Open paths in the terminal from universal actions.", glyph: "T" },
];

pub fn enabled(id: &str) -> bool {
    let cfg = Config::load();
    match cfg.features_enabled.get(id) {
        Some(v) => *v,
        None => true, // default: everything on (matches Alfred fresh install)
    }
}

pub fn list() -> Value {
    let items: Vec<Value> = FEATURES
        .iter()
        .map(|f| {
            json!({
                "id": f.id,
                "name": f.name,
                "desc": f.desc,
                "glyph": f.glyph,
                "enabled": enabled(f.id),
            })
        })
        .collect();
    Value::Array(items)
}

pub fn patch(id: &str, on: bool) -> Result<(), String> {
    if !FEATURES.iter().any(|f| f.id == id) {
        return Err(format!("unknown feature: {id}"));
    }
    let mut cfg = Config::load();
    cfg.features_enabled.insert(id.to_string(), on);
    cfg.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_unique_and_defaults_enabled() {
        let mut seen = std::collections::HashSet::new();
        for f in FEATURES {
            assert!(seen.insert(f.id), "dup feature id {}", f.id);
        }
        let doc = list();
        assert_eq!(doc.as_array().unwrap().len(), FEATURES.len());
        for it in doc.as_array().unwrap() {
            let id = it["id"].as_str().unwrap();
            assert!(it["enabled"].as_bool().unwrap_or(true), "{id} should default enabled");
        }
    }

    #[test]
    fn patch_unknown_id_fails() {
        assert!(patch("nope", true).is_err());
    }
}
