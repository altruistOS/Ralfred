//! Large Type. Evidence: AlfredFeatureLargeType(Config), AlfredLargeTypeView,
//! AlfredUniversalActionTextShowAsLargeType, "Cleaning up Large Type view",
//! "Found a large type window to close" — shows the selection in giant type.
//!
//! Model: `lt <text>` (or the universal action) pushes the text to the GUI's
//! large-type overlay (a div rendered by engine HTML in big type, hidden by
//! Escape/timeout). Without a GUI session, CLI prints it plainly. The pure
//! formatting/scaling logic lives here and is unit-tested.

use crate::search::{Candidate, Kind};

/// Font size scales down with length, like Alfred's auto-fit.
pub fn font_size(text: &str) -> usize {
    let len = text.trim().chars().count();
    if len <= 4 {
        180
    } else if len <= 10 {
        110
    } else if len <= 24 {
        64
    } else if len <= 60 {
        40
    } else {
        24
    }
}

/// `lt <text>` route candidate.
pub fn candidates(q: &str) -> Vec<Candidate> {
    let q = q.trim();
    let (kw, rest) = match q.split_once(' ') {
        Some(x) => x,
        None => (q, ""),
    };
    if kw != "lt" && kw != "largetype" {
        return Vec::new();
    }
    if rest.trim().is_empty() {
        return vec![Candidate {
            title: "Large Type".into(),
            subtitle: "type `lt <text>` to show it in large type".into(),
            path: None,
            score: 866.0,
            kind: Kind::Action,
            action: None,
        }];
    }
    vec![Candidate {
        title: format!("Large Type: {}", rest.trim()),
        subtitle: "shows selection in giant type · esc to dismiss".into(),
        path: None,
        score: 892.0,
        kind: Kind::Action,
        action: Some(format!("largetype:{}", rest.trim())),
    }]
}

/// Execute a `largetype:` action — via env var consumed by the GUI overlay.
pub fn run(text: &str) -> std::io::Result<()> {
    // GUI overlay picks this up through LT_HOOK_FILE when set; CLI fallback
    // prints the text so headless sessions still "work".
    if let Ok(f) = std::env::var("LT_HOOK_FILE") {
        let _ = std::fs::write(f, text);
        return Ok(());
    }
    println!("{}", text);
    Ok(())
}
