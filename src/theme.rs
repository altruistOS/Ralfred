//! Theme system. Evidence: AlfredTheme/AlfredThemeManager classes +
//! alfred_theme_background / alfred_theme_selection_background keys +
//! desiredThemeUID/activeTheme caching in the binary. Alfred stores themes
//! as imported .alfredtheme packages; our clean-room keeps the same *model*
//! (named theme, uid, active selection, custom themes on disk) with our own
//! JSON representation.
//!
//! A theme colors the GUI panel. Themes live at <home>/themes/<uid>.json;
//! the active uid persists in config.theme.

use crate::config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Theme {
    pub name: String,
    pub uid: String,
    /// panel background (RRGGBB + optional alpha hex)
    pub background: String,
    /// inner search-field well (Alfred's field material sits darker)
    #[serde(default = "Theme::default_field")]
    pub field: String,
    pub text: String,
    pub subtext: String,
    pub selection_background: String,
    pub selection_text: String,
    pub border: String,
}

impl Theme {
    pub fn default_field() -> String {
        "111010F8".into()
    }

    pub fn builtin(name: &str) -> Option<Theme> {
        let t = match name {
            "alfred-dark" => Theme {
                name: "Alfred Modern Dark".into(),
                uid: "alfred-dark".into(),
                // measured from Alfred 5 screenshots (dark mode)
                background: "171516F0".into(),
                field: "111010F8".into(),
                text: "ffffff".into(),
                subtext: "d4e3e4".into(),
                selection_background: "2f767c".into(),
                selection_text: "ffffff".into(),
                border: "2a2a2c80".into(),
            },
            "alfred-light" => Theme {
                name: "Alfred Modern Light".into(),
                uid: "alfred-light".into(),
                background: "fcfcfcEE".into(),
                field: "f1f2f2F8".into(),
                text: "202020".into(),
                subtext: "6f6f6f".into(),
                selection_background: "2f767c".into(),
                selection_text: "ffffff".into(),
                border: "d0d0d080".into(),
            },
            "midnight" => Theme {
                name: "Midnight".into(),
                uid: "midnight".into(),
                background: "0d1117EE".into(),
                field: "090c10F8".into(),
                text: "e6edf3".into(),
                subtext: "8b949e".into(),
                selection_background: "1f6febC0".into(),
                selection_text: "ffffff".into(),
                border: "30363d80".into(),
            },
            _ => return None,
        };
        Some(t)
    }

    pub fn builtin_list() -> Vec<(&'static str, &'static str)> {
        vec![
            ("alfred-dark", "Alfred Dark (default)"),
            ("alfred-light", "Alfred Light"),
            ("midnight", "Midnight"),
        ]
    }

    pub fn themes_dir() -> PathBuf {
        config::app_support_dir().join("themes")
    }

    /// Resolve uid -> theme: custom file first, then builtin.
    /// Frosted-glass variant: lighten the alpha so macOS vibrancy (the
    /// NSVisualEffectView behind the webview) shows through the panel chrome.
    /// With translucency off the original near-opaque colors are returned.
    pub fn applied(mut self, glass: bool) -> Theme {
        if glass {
            if !self.background.contains('+') {
                let base = trim_alpha(&self.background);
                self.background = format!("{}96", base); // ~59% over the blur
            }
            if !self.field.contains('+') {
                let base = trim_alpha(&self.field);
                self.field = format!("{}66", base); // field well: even more glass
            }
        }
        self
    }

    pub fn resolve(uid: &str) -> Theme {
        let p = Self::themes_dir().join(format!("{}.json", uid));
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(t) = serde_json::from_str::<Theme>(&s) {
                return t;
            }
        }
        Self::builtin(uid).unwrap_or_else(|| Self::builtin("alfred-dark").unwrap())
    }

    /// all known themes (builtin + custom on disk)
    pub fn all() -> Vec<Theme> {
        let mut v: Vec<Theme> = Self::builtin_list()
            .iter()
            .filter_map(|(u, _)| Self::builtin(u))
            .collect();
        if let Ok(rd) = std::fs::read_dir(Self::themes_dir()) {
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) == Some("json") {
                    if let Ok(s) = std::fs::read_to_string(&path) {
                        if let Ok(t) = serde_json::from_str::<Theme>(&s) {
                            if !v.iter().any(|b| b.uid == t.uid) {
                                v.push(t);
                            }
                        }
                    }
                }
            }
        }
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// inject theme colors into an HTML template with {{placeholders}}
    pub fn apply_html(&self, html: &str) -> String {
        html.replace("{{BG}}", &self.background)
            .replace("{{FIELD}}", &self.field)
            .replace("{{TEXT}}", &self.text)
            .replace("{{SUBTEXT}}", &self.subtext)
            .replace("{{SELBG}}", &self.selection_background)
            .replace("{{SELTEXT}}", &self.selection_text)
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(Self::themes_dir())?;
        let p = Self::themes_dir().join(format!("{}.json", self.uid));
        std::fs::write(&p, serde_json::to_string_pretty(self).unwrap())
    }
}

/// CLI surface: list / set / show / new
pub fn cli(args: &[String]) {
    let mut cfg = config::Config::load();
    match args.first().map(|s| s.as_str()) {
        None | Some("list") => {
            for t in Theme::all() {
                let star = if cfg.theme == t.uid { "* " } else { "  " };
                println!("{}{} ({})", star, t.name, t.uid);
            }
        }
        Some("set") => {
            let uid = args.get(1).map(|s| s.as_str()).unwrap_or("");
            if Theme::builtin(uid).is_none()
                && !Theme::themes_dir().join(format!("{}.json", uid)).exists()
            {
                eprintln!("unknown theme: {uid} (try `ralfred theme list`)");
                std::process::exit(1);
            }
            cfg.theme = uid.to_string();
            let _ = cfg.save();
            println!("theme set to {} — {}", Theme::resolve(uid).name, uid);
        }
        Some("show") => {
            let uid = args.get(1).map(|s| s.as_str()).unwrap_or(&cfg.theme);
            let t = Theme::resolve(uid);
            println!("{} ({}):", t.name, t.uid);
            println!("  background = #{}", t.background);
            println!("  field      = #{}", t.field);
            println!("  text       = #{}", t.text);
            println!("  subtext    = #{}", t.subtext);
            println!("  selection  = #{}", t.selection_background);
        }
        Some("new") => {
            // ralfred theme new <uid> --name <Name> [--bg RRGGBBAA ...]
            if args.len() < 2 {
                eprintln!("usage: ralfred theme new <uid> [--name <name>] [--bg <hex>] [--text <hex>] [--sel <hex>]");
                std::process::exit(1);
            }
            let uid = args[1].clone();
            let mut t = Theme::builtin("alfred-dark")
                .unwrap()
                .clone();
            t.uid = uid.clone();
            t.name = uid.clone();
            let mut i = 2;
            while i < args.len() {
                let key = args[i].as_str();
                let val = args.get(i + 1).cloned().unwrap_or_default();
                match key {
                    "--name" => t.name = val,
                    "--bg" => t.background = val,
                    "--text" => t.text = val,
                    "--subtext" => t.subtext = val,
                    "--sel" => {
                        t.selection_background = val;
                        t.selection_text = "ffffff".into();
                    }
                    _ => {
                        eprintln!("unknown option {key}");
                        std::process::exit(1);
                    }
                }
                i += 2;
            }
            if !t.background.chars().all(|c| c.is_ascii_hexdigit()) {
                eprintln!("bg must be hex RRGGBB or RRGGBBAA");
                std::process::exit(1);
            }
            t.save().unwrap();
            println!("created theme {} ({})", t.name, t.uid);
        }
        _ => {
            eprintln!("usage: ralfred theme [list|set <uid>|show [uid]|new <uid> opts]");
            std::process::exit(1);
        }
    }
}

fn trim_alpha(hex: &str) -> &str {
    let h = hex.trim();
    if h.len() == 8 { &h[..6] } else { h }
}
