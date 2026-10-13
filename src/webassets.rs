//! Embeds the built React front end (web/dist) and serves it through the
//! existing `ralfred://` custom protocol. When the bundle is absent (placeholder
//! from the repo skeleton) callers fall back to the legacy inline engine page.

use crate::theme::Theme;

pub const INDEX: &str = include_str!("../web/dist/index.html");
pub const BUNDLE: &[u8] = include_bytes!("../web/dist/bundle.js");

/// True when a real React bundle was embedded (placeholder is tiny).
pub fn has_real_assets() -> bool {
    BUNDLE.len() > 4096
}

/// index.html with `__THEME__` replaced by the live theme color object.
pub fn index_with_theme(t: &Theme) -> String {
    let j = format!(
        "{{\"BG\":\"{}\",\"FIELD\":\"{}\",\"TEXT\":\"{}\",\"SUBTEXT\":\"{}\",\"SELBG\":\"{}\"}}",
        t.background.trim_start_matches('#'),
        t.field.trim_start_matches('#'),
        t.text.trim_start_matches('#'),
        t.subtext.trim_start_matches('#'),
        t.selection_background.trim_start_matches('#')
    );
    INDEX.replace("__THEME__", &j)
}

#[cfg(test)]
mod tests {
    #[test]
    fn index_placeholder_is_sane() {
        // skeleton ships a placeholder; real bundles are >=100KB
        assert!(super::INDEX.contains("__THEME__"));
        assert!(super::INDEX.contains("bundle.js"));
    }
}
