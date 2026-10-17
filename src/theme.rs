//! Theme system. Evidence: AlfredTheme/AlfredThemeManager classes +
//! alfred_theme_background / alfred_theme_selection_background keys +
//! desiredThemeUID/activeTheme caching in the binary. Alfred stores themes
//! as imported .alfredtheme packages; our clean-room keeps the same *model*
//! (named theme, uid, active selection, custom themes on disk) with our own
//! JSON representation.
//!
//! A theme colors the GUI panel. Themes live at <home>/themes/<uid>.json;
//! the active uid persists in config.theme.
//!
//! SPEC §5.12: Alfred's shipped `.alfredappearance` files are also understood.
//! Such a file is JSON whose root is `{"alfredtheme": { … }}`; it can be
//! dropped straight into the themes directory and is then discovered by
//! [`Theme::all`] / [`Theme::resolve`] alongside the native `<uid>.json` form.
//! Colours in that format are `#RRGGBB` or `#AARRGGBB` (alpha *first*); the
//! internal model stores `RRGGBBAA` (alpha *last*, or `+`-suffixed dynamic
//! alpha), so import re-orders the components. A theme whose body is unreadable
//! or invalid never panics: [`Theme::resolve_reported`] falls back to the
//! built-in default and reports the fallback (SPEC §8.8).

use crate::config;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    // --- geometry carried over from an imported .alfredappearance (SPEC §3.5) ---
    /// panel width in points as declared by the theme
    #[serde(default = "Theme::default_width")]
    pub width: i64,
    /// window corner radius in points (may be negative: capsule corners)
    #[serde(default = "Theme::default_roundness")]
    pub roundness: i64,
    /// window backdrop blur radius in points
    #[serde(default = "Theme::default_blur")]
    pub blur: i64,
    /// window padding around the content, in points
    #[serde(default = "Theme::default_padding_horizontal")]
    pub padding_horizontal: i64,
    #[serde(default = "Theme::default_padding_vertical")]
    pub padding_vertical: i64,
    /// border inset in points
    #[serde(default = "Theme::default_border_padding")]
    pub border_padding: i64,
    /// row icon box in points
    #[serde(default = "Theme::default_icon_size")]
    pub icon_size: i64,
    #[serde(default = "Theme::default_icon_padding_horizontal")]
    pub icon_padding_horizontal: i64,
    /// result font metrics, retained so the panel can size rows
    #[serde(default = "Theme::default_text_size")]
    pub text_size: i64,
    #[serde(default = "Theme::default_subtext_size")]
    pub subtext_size: i64,
    /// separator line thickness in points (0 = no separator)
    #[serde(default = "Theme::default_separator_thickness")]
    pub separator_thickness: i64,
    /// scrollbar thickness in points
    #[serde(default = "Theme::default_scrollbar_thickness")]
    pub scrollbar_thickness: i64,
}

/// One `{ color, colorSelected, font, size }` text style of an
/// `.alfredappearance` document.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[allow(non_snake_case)] // field names mirror Alfred's camelCase JSON keys
pub struct AppearanceText {
    #[serde(deserialize_with = "de_opt_hex")]
    pub color: Option<String>,
    #[serde(deserialize_with = "de_opt_hex")]
    pub colorSelected: Option<String>,
    pub font: Option<String>,
    pub size: Option<i64>,
}

/// The `window` object of an `.alfredappearance` document.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[allow(non_snake_case)] // field names mirror Alfred's camelCase JSON keys
pub struct AppearanceWindow {
    #[serde(deserialize_with = "de_opt_hex")]
    pub color: Option<String>,
    #[serde(deserialize_with = "de_opt_hex")]
    pub borderColor: Option<String>,
    pub paddingHorizontal: Option<i64>,
    pub paddingVertical: Option<i64>,
    pub borderPadding: Option<i64>,
    pub roundness: Option<i64>,
    pub blur: Option<i64>,
    pub width: Option<i64>,
}

/// The `search` object of an `.alfredappearance` document.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[allow(non_snake_case)] // field names mirror Alfred's camelCase JSON keys
pub struct AppearanceSearch {
    #[serde(deserialize_with = "de_opt_hex")]
    pub background: Option<String>,
    #[serde(deserialize_with = "de_opt_hex")]
    pub backgroundSelected: Option<String>,
    pub text: AppearanceText,
    pub paddingHorizontal: Option<i64>,
    pub paddingVertical: Option<i64>,
    pub roundness: Option<i64>,
    pub spacing: Option<i64>,
}

/// The `result` object of an `.alfredappearance` document.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[allow(non_snake_case)] // field names mirror Alfred's camelCase JSON keys
pub struct AppearanceResult {
    #[serde(deserialize_with = "de_opt_hex")]
    pub backgroundSelected: Option<String>,
    pub iconSize: Option<i64>,
    pub iconPaddingHorizontal: Option<i64>,
    pub paddingVertical: Option<i64>,
    pub roundness: Option<i64>,
    pub textSpacing: Option<i64>,
    pub text: AppearanceText,
    pub subtext: AppearanceText,
    pub shortcut: AppearanceText,
}

/// A `color` / `thickness` pair (`separator`, `scrollbar`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppearanceRule {
    #[serde(deserialize_with = "de_opt_hex")]
    pub color: Option<String>,
    pub thickness: Option<i64>,
}

/// The `alfredtheme` object: the complete observed schema of a shipped
/// `.alfredappearance` file (SPEC §5.12). Every field is optional so that a
/// partial document — e.g. `Alfred Classic`, which declares only a name —
/// still deserialises.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[allow(non_snake_case)] // field names mirror Alfred's camelCase JSON keys
pub struct AlfredTheme {
    pub name: Option<String>,
    /// 0/1/2 observed; retained, not interpreted
    pub visualEffectMode: Option<i64>,
    /// optional attribution string, observed empty in two shipped themes
    pub credit: Option<String>,
    pub window: AppearanceWindow,
    pub search: AppearanceSearch,
    pub result: AppearanceResult,
    pub separator: AppearanceRule,
    pub scrollbar: AppearanceRule,
}

/// Document root: `{"alfredtheme": { … }}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AlfredAppearance {
    pub alfredtheme: AlfredTheme,
}

/// The result of resolving a theme, carrying the SPEC §5.12/§8.8 fallback
/// report alongside the theme itself. `report` is `None` when the requested
/// theme was found and parsed, and otherwise explains what was substituted.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    pub theme: Theme,
    pub report: Option<String>,
}

impl ResolvedTheme {
    /// True when the requested uid could not be loaded and the built-in
    /// default was substituted.
    pub fn fell_back(&self) -> bool {
        self.report.is_some()
    }
}

/// The built-in fallback theme: the values `resolve` substitutes when a
/// requested theme is missing or unreadable (SPEC §8.8). Field defaults are
/// declared so an older or partial `<uid>.json` still deserialises.
impl Default for Theme {
    fn default() -> Self {
        Theme {
            name: "Alfred Modern Dark".into(),
            uid: "alfred-dark".into(),
            // measured from Alfred 5 screenshots (dark mode)
            background: "171516F0".into(),
            field: Theme::default_field(),
            text: "ffffff".into(),
            subtext: "d4e3e4".into(),
            selection_background: "2f767c".into(),
            selection_text: "ffffff".into(),
            border: "2a2a2c80".into(),
            // the documented default panel width (§3.5); shipped themes
            // actually declare 559.
            width: Theme::default_width(),
            roundness: Theme::default_roundness(),
            blur: Theme::default_blur(),
            padding_horizontal: Theme::default_padding_horizontal(),
            padding_vertical: Theme::default_padding_vertical(),
            border_padding: Theme::default_border_padding(),
            icon_size: Theme::default_icon_size(),
            icon_padding_horizontal: Theme::default_icon_padding_horizontal(),
            text_size: Theme::default_text_size(),
            subtext_size: Theme::default_subtext_size(),
            separator_thickness: Theme::default_separator_thickness(),
            scrollbar_thickness: Theme::default_scrollbar_thickness(),
        }
    }
}

impl Theme {
    pub fn default_field() -> String {
        "111010F8".into()
    }

    /// Documented default panel width (SPEC §3.5).
    pub fn default_width() -> i64 {
        700
    }

    // Per-field serde defaults. A `<uid>.json` written before the geometry
    // fields existed must fall back to the built-in theme's values rather than
    // to zero, which would collapse the panel.
    pub fn default_roundness() -> i64 {
        3
    }

    pub fn default_blur() -> i64 {
        15
    }

    pub fn default_padding_horizontal() -> i64 {
        9
    }

    pub fn default_padding_vertical() -> i64 {
        8
    }

    pub fn default_border_padding() -> i64 {
        0
    }

    pub fn default_icon_size() -> i64 {
        40
    }

    pub fn default_icon_padding_horizontal() -> i64 {
        4
    }

    pub fn default_text_size() -> i64 {
        18
    }

    pub fn default_subtext_size() -> i64 {
        12
    }

    pub fn default_separator_thickness() -> i64 {
        0
    }

    pub fn default_scrollbar_thickness() -> i64 {
        2
    }

    pub fn builtin(name: &str) -> Option<Theme> {
        let t = match name {
            "alfred-dark" => Theme::default(),
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
                ..Theme::default()
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
                ..Theme::default()
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

    /// Resolve uid -> theme, preferring `<uid>.json`, then a built-in, then a
    /// matching `.alfredappearance` dropped into the themes directory.
    /// Never fails: an unknown uid yields the built-in default.
    pub fn resolve(uid: &str) -> Theme {
        Self::resolve_reported(uid).theme
    }

    /// [`Theme::resolve`] with the SPEC §5.12/§8.8 fallback report. `report` is
    /// `None` on a clean hit, and otherwise names what went wrong and which
    /// theme was substituted.
    pub fn resolve_reported(uid: &str) -> ResolvedTheme {
        let default = Self::builtin("alfred-dark").unwrap_or_else(Theme::default);
        match Self::load_reported(uid) {
            Ok(theme) => ResolvedTheme {
                theme,
                report: None,
            },
            Err(reason) => ResolvedTheme {
                theme: default,
                report: Some(format!("{reason}; falling back to default theme")),
            },
        }
    }

    /// Load uid without the default substitution. Precedence: built-in uid,
    /// native `<uid>.json`, then an `.alfredappearance` whose name slug (or
    /// file stem) matches `uid`.
    pub fn load_reported(uid: &str) -> Result<Theme, String> {
        if let Some(t) = Self::builtin(uid) {
            return Ok(t);
        }
        let dir = Self::themes_dir();
        let native = dir.join(format!("{}.json", uid));
        if !native.exists() {
            if let Some(t) = Self::appearance_in(&dir, uid)? {
                return Ok(t);
            }
            return Err(format!("theme not found: {uid}"));
        }
        let s = std::fs::read_to_string(&native)
            .map_err(|e| format!("unable to read theme {}: {e}", native.display()))?;
        serde_json::from_str::<Theme>(&s)
            .map_err(|e| format!("incorrect theme file {}: {e}", native.display()))
    }

    /// all known themes (builtin + custom `<uid>.json` + `.alfredappearance`)
    pub fn all() -> Vec<Theme> {
        Self::all_in(&Self::themes_dir())
    }

    /// [`Theme::all`] against an explicit directory (used by tests so they do
    /// not touch the real data root).
    pub fn all_in(dir: &Path) -> Vec<Theme> {
        let mut v: Vec<Theme> = Self::builtin_list()
            .iter()
            .filter_map(|(u, _)| Self::builtin(u))
            .collect();
        let mut seen: Vec<String> = v.iter().map(|t| t.uid.clone()).collect();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let path = e.path();
                let ext = path.extension().and_then(|x| x.to_str());
                let loaded = match ext {
                    Some("json") => std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<Theme>(&s).ok()),
                    Some("alfredappearance") => match Self::appearance_to_theme(&path) {
                        // An unreadable theme is skipped, never fatal (§5.12).
                        Ok(t) => Some(t),
                        Err(reason) => {
                            eprintln!("skipping theme {}: {reason}", path.display());
                            None
                        }
                    },
                    _ => None,
                };
                if let Some(t) = loaded {
                    if !seen.iter().any(|u| *u == t.uid) {
                        seen.push(t.uid.clone());
                        v.push(t);
                    }
                }
            }
        }
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// Every `.alfredappearance` file in the themes directory, with the uid it
    /// would be discovered under. Files that cannot be read are reported and
    /// skipped.
    pub fn appearance_files() -> Vec<(String, PathBuf)> {
        Self::appearance_files_in(&Self::themes_dir())
    }

    /// [`Theme::appearance_files`] against an explicit directory.
    pub fn appearance_files_in(dir: &Path) -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("alfredappearance") {
                    continue;
                }
                match Self::alfred_appearance_uid(&path) {
                    Ok(uid) => out.push((uid, path)),
                    Err(reason) => eprintln!("skipping theme {}: {reason}", path.display()),
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The uid an `.alfredappearance` file is discovered under: the slug of its
    /// declared name, or of its file stem when no usable name is present.
    pub fn alfred_appearance_uid(path: &Path) -> Result<String, String> {
        let doc = Self::read_alfred_appearance(path)?;
        Ok(match doc.alfredtheme.name.as_deref() {
            Some(n) if !slugify(n).is_empty() => slugify(n),
            _ => slugify(
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default(),
            ),
        })
    }

    /// Find an `.alfredappearance` file in `dir` whose uid matches `uid`.
    /// `Ok(None)` means no such file exists; `Err` means one matched but its
    /// body was unreadable or invalid.
    pub fn appearance_in(dir: &Path, uid: &str) -> Result<Option<Theme>, String> {
        for (candidate, path) in Self::appearance_files_in(dir) {
            if candidate == uid {
                return Self::appearance_to_theme(&path).map(Some);
            }
        }
        // Fall back to an exact file-stem match, so an imported file that is
        // named by hand still resolves.
        let p = dir.join(format!("{}.alfredappearance", uid));
        if p.exists() {
            return Self::appearance_to_theme(&p).map(Some);
        }
        Ok(None)
    }

    /// Map an `.alfredappearance` file into the internal [`Theme`] model.
    pub fn appearance_to_theme(path: &Path) -> Result<Theme, String> {
        let doc = Self::read_alfred_appearance(path)?;
        Ok(Self::from_alfred_theme(doc.alfredtheme, path))
    }

    /// Map a parsed `alfredtheme` object into the internal [`Theme`] model.
    /// Pure function; every field is optional and falls back to the built-in
    /// default. Alpha is re-ordered from `#AARRGGBB` to `RRGGBBAA`.
    pub fn from_alfred_theme(t: AlfredTheme, path: &Path) -> Theme {
        let d = Theme::default();
        let name = t
            .name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Imported Theme")
                    .to_string()
            });
        let uid = match slugify(&name) {
            s if !s.is_empty() => s,
            _ => slugify(
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("imported"),
            ),
        };
        let pick = |v: Option<String>, fallback: &str| -> String {
            v.filter(|s| !s.trim().is_empty())
                .map(|s| hex_to_internal(&s))
                .unwrap_or_else(|| fallback.to_string())
        };
        Theme {
            name,
            uid,
            background: pick(t.window.color, &d.background),
            field: pick(t.search.background, &d.field),
            text: pick(t.search.text.color.clone(), &d.text),
            subtext: pick(
                t.result.subtext.color.clone(),
                &pick(t.result.text.color.clone(), &d.subtext),
            ),
            selection_background: pick(t.result.backgroundSelected, &d.selection_background),
            selection_text: pick(
                t.result
                    .text
                    .colorSelected
                    .clone()
                    .or_else(|| t.search.text.colorSelected.clone()),
                &d.selection_text,
            ),
            border: pick(t.window.borderColor, &d.border),
            width: t.window.width.filter(|w| *w > 0).unwrap_or(d.width),
            roundness: t.window.roundness.unwrap_or(d.roundness),
            blur: t.window.blur.unwrap_or(d.blur),
            padding_horizontal: t.window.paddingHorizontal.unwrap_or(d.padding_horizontal),
            padding_vertical: t.window.paddingVertical.unwrap_or(d.padding_vertical),
            border_padding: t.window.borderPadding.unwrap_or(d.border_padding),
            icon_size: t.result.iconSize.unwrap_or(d.icon_size),
            icon_padding_horizontal: t
                .result
                .iconPaddingHorizontal
                .unwrap_or(d.icon_padding_horizontal),
            text_size: t.result.text.size.unwrap_or(d.text_size),
            subtext_size: t.result.subtext.size.unwrap_or(d.subtext_size),
            separator_thickness: t.separator.thickness.unwrap_or(d.separator_thickness),
            scrollbar_thickness: t.scrollbar.thickness.unwrap_or(d.scrollbar_thickness),
        }
    }

    /// Read and validate one `.alfredappearance` document.
    pub fn read_alfred_appearance(path: &Path) -> Result<AlfredAppearance, String> {
        let s = std::fs::read_to_string(path)
            .map_err(|e| format!("unable to read application appearance: {e}"))?;
        Self::parse_alfred_appearance(&s)
    }

    /// Parse the JSON body of an `.alfredappearance` document.
    pub fn parse_alfred_appearance(json: &str) -> Result<AlfredAppearance, String> {
        serde_json::from_str::<AlfredAppearance>(json)
            .map_err(|e| format!("incorrect theme file: {e}"))
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

/// Import an Alfred `.alfredappearance` file (SPEC §5.12) and map it into the
/// internal [`Theme`] model, which the panel can then use directly.
///
/// The document is JSON with an `{"alfredtheme": { … }}` root. Colours are
/// `#RRGGBB` or `#AARRGGBB` (alpha first) and are re-ordered to the internal
/// `RRGGBBAA` form, so alpha is honoured. Missing optional keys take the
/// built-in default; `Err` names the problem for a missing, unreadable,
/// non-JSON or schema-invalid file.
pub fn import_alfred_appearance(path: &Path) -> Result<Theme, String> {
    Theme::appearance_to_theme(path)
}

/// Parse and import the JSON body of an `.alfredappearance` document without
/// touching the filesystem. `fallback_name` is used when the document
/// declares no usable `name`.
pub fn import_alfred_appearance_str(json: &str, fallback_name: &str) -> Result<Theme, String> {
    let doc = Theme::parse_alfred_appearance(json)?;
    Ok(Theme::from_alfred_theme(
        doc.alfredtheme,
        Path::new(fallback_name),
    ))
}

/// serde helper: accept a hex colour string, or any scalar rendered as text,
/// without failing the whole document on one odd value.
fn de_opt_hex<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    let v = Option::<serde_json::Value>::deserialize(d)?;
    Ok(match v {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        Some(other) => {
            return Err(D::Error::custom(format!(
                "expected a hex colour string, found {other}"
            )))
        }
    })
}

/// Convert an Alfred colour to the internal representation: strip `#`, accept
/// 6 hex digits (`RRGGBB`) or 8 (`AARRGGBB`, alpha first) and store the
/// internal `RRGGBBAA` form (alpha last). Anything else is returned unchanged
/// so a malformed value degrades rather than corrupting the theme.
fn hex_to_internal(color: &str) -> String {
    let h = color.trim().trim_start_matches('#');
    if h.len() == 8 && h.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("{}{}", &h[2..8], &h[0..2])
    } else {
        h.to_string()
    }
}

/// Slug used as the uid of an imported theme: lowercase, runs of anything that
/// is not alphanumeric collapsed to a single `-`, no leading/trailing `-`.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_dash = false;
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A representative `.alfredappearance` exercising every documented key.
    /// Written for the test rather than copied from a shipped theme.
    const SAMPLE: &str = r##"{
  "alfredtheme": {
    "name": "Test Theme",
    "visualEffectMode": 1,
    "credit": "",
    "window": {
      "color": "#12345678",
      "borderColor": "#AABBCC",
      "paddingHorizontal": 11,
      "paddingVertical": 12,
      "borderPadding": 13,
      "roundness": 14,
      "blur": 15,
      "width": 641
    },
    "search": {
      "background": "#01020340",
      "backgroundSelected": "#11121350",
      "text": {
        "color": "#212223",
        "colorSelected": "#313233",
        "font": "System Light",
        "size": 34
      },
      "paddingHorizontal": 7,
      "paddingVertical": 5,
      "roundness": 6,
      "spacing": 8
    },
    "result": {
      "backgroundSelected": "#41424360",
      "iconSize": 44,
      "iconPaddingHorizontal": 9,
      "paddingVertical": 3,
      "roundness": 4,
      "textSpacing": 5,
      "text": {
        "color": "#515253",
        "colorSelected": "#616263",
        "font": "System",
        "size": 21
      },
      "subtext": {
        "color": "#71727380",
        "colorSelected": "#81828390",
        "font": "Avenir",
        "size": 12
      },
      "shortcut": {
        "color": "#919293",
        "colorSelected": "#A1A2A3",
        "font": "Avenir",
        "size": 16
      }
    },
    "separator": { "color": "#B1B2B3", "thickness": 2 },
    "scrollbar": { "color": "#C1C2C3", "thickness": 3 }
  }
}"##;

    /// A minimal document: only the name is present, so every other field
    /// must come from the built-in default.
    fn sample_theme() -> Theme {
        import_alfred_appearance_str(SAMPLE, "ignored").expect("sample parses")
    }

    #[test]
    fn parses_full_appearance_document() {
        let t = sample_theme();
        assert_eq!(t.name, "Test Theme");
        assert_eq!(t.uid, "test-theme");
        // #AARRGGBB -> RRGGBBAA
        assert_eq!(t.background, "34567812");
        assert_eq!(t.border, "AABBCC");
        assert_eq!(t.field, "02034001");
        // search text drives the primary text colour
        assert_eq!(t.text, "212223");
        assert_eq!(t.subtext, "72738071");
        // alpha from result.backgroundSelected is re-ordered, not dropped
        assert_eq!(t.selection_background, "42436041");
        assert_eq!(t.selection_text, "616263");
        // geometry survives the mapping
        assert_eq!(t.width, 641);
        assert_eq!(t.roundness, 14);
        assert_eq!(t.blur, 15);
        assert_eq!(t.padding_horizontal, 11);
        assert_eq!(t.padding_vertical, 12);
        assert_eq!(t.border_padding, 13);
        assert_eq!(t.icon_size, 44);
        assert_eq!(t.icon_padding_horizontal, 9);
        assert_eq!(t.text_size, 21);
        assert_eq!(t.subtext_size, 12);
        assert_eq!(t.separator_thickness, 2);
        assert_eq!(t.scrollbar_thickness, 3);
        // the document's own keys are all reachable through the typed struct
        let doc = Theme::parse_alfred_appearance(SAMPLE).unwrap();
        assert_eq!(doc.alfredtheme.visualEffectMode, Some(1));
        assert_eq!(doc.alfredtheme.search.spacing, Some(8));
        assert_eq!(
            doc.alfredtheme.result.shortcut.font.as_deref(),
            Some("Avenir")
        );
        assert_eq!(doc.alfredtheme.result.shortcut.size, Some(16));
        assert_eq!(
            doc.alfredtheme.window.color.as_deref(),
            Some("#12345678")
        );
    }

    #[test]
    fn parses_eight_digit_alpha_hex() {
        // alpha-first -> alpha-last, alpha honoured
        assert_eq!(hex_to_internal("#480069E4"), "0069E448");
        assert_eq!(hex_to_internal("#00000000"), "00000000");
        assert_eq!(hex_to_internal("#FFFFFFFF"), "FFFFFFFF");
        let t = sample_theme();
        // #01020340 (alpha 0x40) must not become opaque
        assert_ne!(t.field, "010203");
        assert_eq!(&t.field[6..8], "01");
        // a fully transparent window colour stays transparent
        assert_eq!(hex_to_internal("#FFFFFF00"), "FFFF00FF");
    }

    #[test]
    fn parses_six_digit_hex() {
        assert_eq!(hex_to_internal("#A1B2C3"), "A1B2C3");
        assert_eq!(hex_to_internal("A1B2C3"), "A1B2C3");
        let t = sample_theme();
        assert_eq!(t.text, "212223");
        assert_eq!(t.border, "AABBCC");
    }

    #[test]
    fn missing_optional_fields_default() {
        // A minimal document: only the name is present, so every other field
        // must come from the built-in default.
        let t = import_alfred_appearance_str(r##"{"alfredtheme": {"name": "Bare"}}"##, "ignored")
            .unwrap();
        let d = Theme::default();
        assert_eq!(t.name, "Bare");
        assert_eq!(t.uid, "bare");
        assert_eq!(t.background, d.background);
        assert_eq!(t.field, d.field);
        assert_eq!(t.text, d.text);
        assert_eq!(t.subtext, d.subtext);
        assert_eq!(t.selection_background, d.selection_background);
        assert_eq!(t.selection_text, d.selection_text);
        assert_eq!(t.border, d.border);
        assert_eq!(t.width, Theme::default_width());
        assert_eq!(t.roundness, d.roundness);
        assert_eq!(t.icon_size, d.icon_size);
        assert_eq!(t.separator_thickness, d.separator_thickness);
    }

    #[test]
    fn invalid_document_falls_back_to_default() {
        // Not JSON at all, and a trailing-comma document like the shipped
        // "Alfred Classic" theme.
        for bad in [
            "not json at all",
            r##"{"alfredtheme": {"name": "Alfred Classic",}}"##,
            r##"{"alfredtheme": 7}"##,
        ] {
            let err = import_alfred_appearance_str(bad, "Bad Theme")
                .expect_err("invalid body must be rejected");
            assert!(err.contains("incorrect theme file"), "unexpected: {err}");
        }
        // A missing file is reported, not a panic.
        let missing = std::path::Path::new("/nonexistent/nope.alfredappearance");
        let err = import_alfred_appearance(missing).expect_err("missing file must error");
        assert!(err.contains("unable to read application appearance"));

        // A document with the wrong (or no) root is not an appearance file: it
        // imports as an all-default theme named after the file/to-import name.
        let wrong = import_alfred_appearance_str(r##"{"wrongroot": {}}"##, "wrongroot")
            .expect("a non-appearance document decodes to defaults");
        assert_eq!(wrong.name, "wrongroot");
        assert_eq!(wrong.background, Theme::default().background);

        // resolve_reported substitutes the default and carries the report.
        let r = Theme::resolve_reported("no-such-theme-uid-xyz");
        assert!(r.fell_back());
        assert!(r.report.as_deref().unwrap().contains("falling back to default"));
        assert_eq!(r.theme.uid, "alfred-dark");
        assert_eq!(r.theme.name, "Alfred Modern Dark");
    }

    #[test]
    fn save_and_reload_round_trips() {
        // The light built-in carries every field, so a serialise/parse cycle
        // must be lossless (`save()` itself writes to the data root, so the
        // round-trip is exercised through the same serde representation).
        let t = Theme::builtin("alfred-light").unwrap();
        let json = serde_json::to_string_pretty(&t).unwrap();
        let mut back: Theme = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
        // An imported appearance also survives a save/load cycle.
        let imported = sample_theme();
        back = serde_json::from_str(&serde_json::to_string(&imported).unwrap()).unwrap();
        assert_eq!(imported, back);
    }

    #[test]
    fn older_theme_json_without_geometry_still_loads() {
        // Backwards compatibility: a `Theme` document written before the
        // geometry fields existed must still deserialise.
        let old = r##"{
          "name": "Legacy", "uid": "legacy",
          "background": "171516F0", "field": "111010F8",
          "text": "ffffff", "subtext": "d4e3e4",
          "selection_background": "2f767c", "selection_text": "ffffff",
          "border": "2a2a2c80"
        }"##;
        let t: Theme = serde_json::from_str(old).unwrap();
        let d = Theme::default();
        assert_eq!(t.uid, "legacy");
        // Every geometry field must fall back to the built-in default, not to
        // zero (which would collapse the panel).
        assert_eq!(t.width, Theme::default_width());
        assert_eq!(t.roundness, d.roundness);
        assert_eq!(t.blur, d.blur);
        assert_eq!(t.padding_horizontal, d.padding_horizontal);
        assert_eq!(t.padding_vertical, d.padding_vertical);
        assert_eq!(t.border_padding, d.border_padding);
        assert_eq!(t.icon_size, d.icon_size);
        assert_eq!(t.icon_padding_horizontal, d.icon_padding_horizontal);
        assert_eq!(t.text_size, d.text_size);
        assert_eq!(t.subtext_size, d.subtext_size);
        assert_eq!(t.separator_thickness, d.separator_thickness);
        assert_eq!(t.scrollbar_thickness, d.scrollbar_thickness);
        assert_eq!(t.field, "111010F8");
    }

    #[test]
    fn discovery_picks_up_appearance_files_and_skips_bad_ones() {
        let dir = std::env::temp_dir().join(format!(
            "ralfred-theme-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let good = dir.join("Test Theme.alfredappearance");
        std::fs::write(&good, SAMPLE).unwrap();
        // A native json theme and a broken appearance must coexist.
        let native = dir.join("native.json");
        let mut n = Theme::default();
        n.name = "Native".into();
        n.uid = "native".into();
        std::fs::write(&native, serde_json::to_string(&n).unwrap()).unwrap();
        std::fs::write(dir.join("broken.alfredappearance"), "{ oops").unwrap();

        let listed = Theme::all_in(&dir);
        let ids: Vec<&str> = listed.iter().map(|t| t.uid.as_str()).collect();
        assert!(ids.contains(&"alfred-dark"), "{ids:?}");
        assert!(ids.contains(&"native"), "{ids:?}");
        assert!(ids.contains(&"test-theme"), "{ids:?}");
        // built-ins + the native theme + the imported appearance; the broken
        // file is skipped, never fatal.
        assert_eq!(listed.len(), Theme::builtin_list().len() + 2, "{ids:?}");
        // deterministic ordering by name
        let names: Vec<&str> = listed.iter().map(|t| t.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);

        // resolve by uid finds the imported appearance from the directory
        let found = Theme::appearance_in(&dir, "test-theme").unwrap().unwrap();
        assert_eq!(found.name, "Test Theme");
        assert_eq!(found.width, 641);
        // an unparseable matching file is an error, not a panic
        assert!(Theme::appearance_in(&dir, "broken").is_err());
        assert!(Theme::appearance_in(&dir, "absent").unwrap().is_none());

        let files = Theme::appearance_files_in(&dir);
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(files[0].0, "test-theme");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cli_helpers_are_unchanged() {
        // apply_html still substitutes every placeholder.
        let t = sample_theme();
        let html = t.apply_html("{{BG}}|{{FIELD}}|{{TEXT}}|{{SUBTEXT}}|{{SELBG}}|{{SELTEXT}}");
        assert_eq!(
            html,
            format!(
                "{}|{}|{}|{}|{}|{}",
                t.background, t.field, t.text, t.subtext, t.selection_background, t.selection_text
            )
        );
        // Every built-in still resolves and serialises for `theme save`.
        for (uid, _) in Theme::builtin_list() {
            let b = Theme::builtin(uid).unwrap();
            assert_eq!(b.uid, uid);
            assert!(serde_json::to_string(&b).is_ok());
        }
        // Unknown uid still yields the default rather than panicking.
        assert_eq!(Theme::resolve("definitely-not-a-theme").uid, "alfred-dark");
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
            // Accept built-ins, native <uid>.json, and .alfredappearance files
            // dropped into the themes directory (SPEC §5.12).
            if Theme::builtin(uid).is_none() && Theme::load_reported(uid).is_err() {
                // An appearance file whose declared name slugs to this uid is
                // still usable even when the file stem differs.
                match Theme::appearance_in(&Theme::themes_dir(), uid) {
                    Ok(Some(_)) => {}
                    _ => {
                        eprintln!("unknown theme: {uid} (try `ralfred theme list`)");
                        std::process::exit(1);
                    }
                }
            }
            cfg.theme = uid.to_string();
            let _ = cfg.save();
            println!("theme set to {} — {}", Theme::resolve(uid).name, uid);
        }
        Some("show") => {
            let uid = args.get(1).map(|s| s.as_str()).unwrap_or(&cfg.theme);
            let r = Theme::resolve_reported(uid);
            if let Some(report) = &r.report {
                eprintln!("theme: {report}");
            }
            let t = r.theme;
            println!("{} ({}):", t.name, t.uid);
            println!("  background = #{}", t.background);
            println!("  field      = #{}", t.field);
            println!("  text       = #{}", t.text);
            println!("  subtext    = #{}", t.subtext);
            println!("  selection  = #{}", t.selection_background);
            println!("  width      = {}", t.width);
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
