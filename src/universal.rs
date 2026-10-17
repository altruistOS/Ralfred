//! Universal Actions — mapped from Alfred's verified `alfred.action.*` string
//! table in the shipped binary. Each action takes (path-or-text) and performs
//! a real macOS operation via open/osascript/FS calls.

use crate::router::Action;

/// Resolve an `alfred.action.*` identifier for `target` into an executable
/// description + implementation. Returns None for unknown identifiers.
pub fn run_action(id: &str, target: &str) -> std::io::Result<()> {
    use std::process::Command;
    // NOACT only suppresses *automatic* side effects (exec_action); actions
    // here are explicitly user-invoked (CLI `ralfred actions` / Tab menu).
    let t = target;
    match id {
        "alfred.action.copy" => {
            let dst = next_available(t)?;
            std::fs::copy(t, &dst)?;
            Ok(())
        }
        "alfred.action.copypath" => crate::router::set_clipboard(t),
        "alfred.action.delete" => {
            Command::new("osascript")
                .arg("-e")
                .arg(format!("tell application \"Finder\" to delete POSIX file \"{t}\""))
                .status()?;
            Ok(())
        }
        "alfred.action.getinfo" => {
            Command::new("osascript")
                .arg("-e")
                .arg(format!("tell application \"Finder\" to get info \"{t}\""))
                .status()?;
            Ok(())
        }
        "alfred.action.move" => {
            // move to home dir (Alfred prompts for destination; we keep it simple)
            let name = std::path::Path::new(t)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if !name.is_empty() && std::path::Path::new(t).exists() {
                let dst = dirs::home_dir().unwrap_or_default().join(name);
                std::fs::rename(t, dst)?;
            }
            Ok(())
        }
        "alfred.action.open" => crate::router::exec_action(Action::Open(t.into())),
        "alfred.action.openterminalhere" => {
            let dir = if std::path::Path::new(t).is_dir() {
                t.to_string()
            } else {
                std::path::Path::new(t)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| t.to_string())
            };
            crate::router::exec_action(Action::OpenTerminal(dir.into()))
        }
        "alfred.action.revealinfinder" => crate::router::exec_action(Action::Reveal(t.into())),
        "alfred.action.showpackagecontents" => {
            let contents = std::path::Path::new(t).join("Contents");
            crate::router::exec_action(Action::Reveal(contents))
        }
        "alfred.action.url.open" => crate::router::exec_action(Action::OpenUrl(t.into())),
        "alfred.action.mixed.email" => crate::router::exec_action(Action::OpenUrl(format!("mailto:{t}"))),
        "alfred.action.browsefolder" => crate::router::exec_action(Action::Open(t.into())),
        "alfred.action.showaslargetype" => {
            // Large Type overlay without GUI: use say-style dialog as fallback.
            Command::new("osascript")
                .arg("-e")
                .arg(format!("display dialog \"{t}\" with title \"Large Type\""))
                .status()?;
            Ok(())
        }
        "alfred.action.copyfile" => run_action("alfred.action.copy", t),
        // Evidence strings: alfred.action.text.openindictionary / dict://{query}
        "alfred.action.text.openindictionary" => {
            crate::router::exec_action(Action::OpenUrl(format!("dict://{}", t)))
        }
        "alfred.action.text.copyasplaintext" => {
            // strip HTML tags if any, then copy
            let plain: String = t
                .split('<')
                .map(|seg| seg.split('>').last().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("");
            crate::router::set_clipboard(&plain)
        }
        "alfred.action.text.pasteasplaintext" => {
            crate::router::set_clipboard(t)?;
            paste_via_cmdv()
        }
        "alfred.action.text.saveassnippet" => {
            // A saved snippet must be reachable by auto-expansion, so it needs a
            // real keyword: `match_expand_rows` ignores keywords shorter than two
            // characters (SPEC §5.5). Derive one from the text rather than
            // storing an empty keyword that can never fire.
            let keyword: String = t
                .split_whitespace()
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            let keyword = if keyword.chars().count() >= 2 {
                keyword
            } else {
                t.chars()
                    .filter(|c| c.is_alphanumeric())
                    .take(8)
                    .collect::<String>()
                    .to_lowercase()
            };
            if keyword.chars().count() < 2 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "cannot derive a snippet keyword from this text",
                ));
            }
            let store = crate::db::Store::snippets();
            let name: String = t.lines().next().unwrap_or("").chars().take(40).collect();
            let uid = store.add_snippet(&keyword, &t, &name);
            println!("saved snippet {uid} ({keyword})");
            Ok(())
        }
        "alfred.action.text.useinalfred" => crate::router::set_clipboard(t),
        "alfred.action.text.searchwithfinder" => {
            // Spotlight-style search inside Finder: focuses finder search? open with query
            std::process::Command::new("osascript")
                .args(["-e", &format!(
                    "tell application \"Finder\" to search (container of window 1) for \"{}\"", t.replace('"', "\"")
                )])
                .status()
                .map(|_| ())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        }
        "alfred.action.url.openin" => crate::router::exec_action(Action::OpenUrl(t.into())),
        "alfred.action.url.usedomainfromurl" => {
            let dom = t.split('/').nth(2).unwrap_or("");
            println!("domain: {}", dom);
            Ok(())
        }
        // Quick Look: evidence generateQuickLookURLForSelectedItem -> qlmanage -p
        "alfred.action.quicklook" => {
            std::process::Command::new("qlmanage")
                .args(["-p", t])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map(|_| ())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        }
        // Unknown identifiers must fail loudly: silently reporting success made
        // a mis-typed or unimplemented action look like it worked.
        other => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unsupported universal action id: {other}"),
        )),
    }
}

/// Human label for an action id (Alfred menu wording).
pub fn label(id: &str) -> &'static str {
    match id {
        "alfred.action.open" => "Open",
        "alfred.action.browsefolder" => "Browse Folder",
        "alfred.action.revealinfinder" => "Show in Finder",
        "alfred.action.openterminalhere" => "Open Terminal Here",
        "alfred.action.copypath" => "Copy File Path",
        "alfred.action.copy" => "Copy File to …",
        "alfred.action.copyfile" => "Copy File",
        "alfred.action.move" => "Move File to …",
        "alfred.action.delete" => "Move to Trash",
        "alfred.action.getinfo" => "Get Info",
        "alfred.action.showpackagecontents" => "Show Package Contents",
        "alfred.action.quicklook" => "Quick Look",
        "alfred.action.mixed.email" => "Compose Email (Mixed)",
        "alfred.action.openwith" => "Open With…",
        "alfred.action.showaslargetype" => "Show As Large Type",
        "alfred.action.url.open" => "Open URL",
        "alfred.action.url.openin" => "Open URL In Default Browser",
        "alfred.action.url.usedomainfromurl" => "Copy Domain",
        "alfred.action.text.openindictionary" => "Dictionary Definition",
        "alfred.action.text.copyasplaintext" => "Copy as Plain Text",
        "alfred.action.text.pasteasplaintext" => "Paste as Plain Text",
        "alfred.action.text.saveassnippet" => "Create Snippet…",
        "alfred.action.text.useinalfred" => "Use As Alfred Query",
        "alfred.action.text.searchwithfinder" => "Search with Finder",
        _ => "Action",
    }
}

/// Actions applicable to `target` — files/folders get the Finder set, http(s)
/// URLs the URL set, anything else the text set. (REA: universal-actions table.)
pub fn applicable(target: &str) -> Vec<(&'static str, &'static str)> {
    let is_url = target.starts_with("http://") || target.starts_with("https://");
    let is_path = !is_url && std::path::Path::new(target).exists();
    let ids: &[&str] = if is_url {
        &["alfred.action.url.open", "alfred.action.url.openin", "alfred.action.url.usedomainfromurl"]
    } else if is_path {
        const F: &[&str] = &[
            "alfred.action.open",
            "alfred.action.browsefolder",
            "alfred.action.revealinfinder",
            "alfred.action.openterminalhere",
            "alfred.action.copypath",
            "alfred.action.copy",
            "alfred.action.move",
            "alfred.action.delete",
            "alfred.action.getinfo",
            "alfred.action.showpackagecontents",
            "alfred.action.quicklook",
        ];
        F
    } else {
        const T: &[&str] = &[
            "alfred.action.text.copyasplaintext",
            "alfred.action.text.pasteasplaintext",
            "alfred.action.text.saveassnippet",
            "alfred.action.text.useinalfred",
            "alfred.action.text.openindictionary",
            "alfred.action.text.searchwithfinder",
        ];
        T
    };
    ids.iter().map(|i| (*i, label(i))).collect()
}

/// Known action ids for discovery (`ralfred actions --list`).
fn paste_via_cmdv() -> std::io::Result<()> {
    use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventFlags};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "no event source"))?;
    if let (Ok(d), Ok(u)) = (
        CGEvent::new_keyboard_event(src.clone(), 9, true),
        CGEvent::new_keyboard_event(src.clone(), 9, false),
    ) {
        d.set_flags(CGEventFlags::CGEventFlagCommand);
        u.set_flags(CGEventFlags::CGEventFlagCommand);
        d.post(CGEventTapLocation::Session);
        u.post(CGEventTapLocation::Session);
    }
    Ok(())
}

pub fn known_actions() -> &'static [&'static str] {
    &[
        "alfred.action.browsefolder",
        "alfred.action.copy",
        "alfred.action.copyfile",
        "alfred.action.copypath",
        "alfred.action.delete",
        "alfred.action.getinfo",
        "alfred.action.mixed.email",
        "alfred.action.move",
        "alfred.action.open",
        "alfred.action.openterminalhere",
        // `alfred.action.openwith` is an observed identifier, but "Open With…"
        // is an interactive application chooser: it has no non-interactive
        // execution, so it is deliberately not advertised (SPEC §5.7 — the
        // offered set must be executable).
        "alfred.action.revealinfinder",
        "alfred.action.showaslargetype",
        "alfred.action.showpackagecontents",
        "alfred.action.url.open",
        "alfred.action.url.openin",
        "alfred.action.url.usedomainfromurl",
        "alfred.action.text.openindictionary",
        "alfred.action.text.copyasplaintext",
        "alfred.action.text.pasteasplaintext",
        "alfred.action.text.saveassnippet",
        "alfred.action.text.useinalfred",
        "alfred.action.text.searchwithfinder",
        "alfred.action.quicklook",
    ]
}

fn next_available(src: &str) -> std::io::Result<std::path::PathBuf> {
    let p = std::path::Path::new(src);
    let dir = p.parent().unwrap_or(p);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = p.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    let mut dst = dir.join(format!("{stem}{ext}"));
    let mut i = 1;
    while dst.exists() {
        dst = dir.join(format!("{stem} {i}{ext}"));
        i += 1;
        if i > 999 {
            break;
        }
    }
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applicable_by_target_kind() {
        // real directory -> Finder set (contains reveal + terminal)
        let dir = std::env::temp_dir();
        let p = dir.to_string_lossy().to_string();
        let ids: Vec<_> = applicable(&p).into_iter().map(|(i, _)| i).collect();
        assert!(ids.contains(&"alfred.action.revealinfinder"));
        assert!(ids.contains(&"alfred.action.openterminalhere"));

        let ids: Vec<_> = applicable("https://example.com/x")
            .into_iter().map(|(i, _)| i).collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"alfred.action.url.open"));

        // plain text -> text set
        let ids: Vec<_> = applicable("hello world")
            .into_iter().map(|(i, _)| i).collect();
        assert!(ids.contains(&"alfred.action.text.copyasplaintext"));
        assert!(!ids.contains(&"alfred.action.delete"));
    }

    #[test]
    fn labels_cover_all_known_ids() {
        for id in known_actions() {
            assert_ne!(label(id), "Action", "missing label for {id}");
        }
    }

    #[test]
    fn unknown_action_id_is_an_error() {
        // A mis-typed or unimplemented id must not report success.
        let e = run_action("alfred.action.doesnotexist", "x").unwrap_err();
        assert!(e.to_string().contains("unsupported"), "{e}");
    }

    #[test]
    fn openwith_is_not_advertised() {
        // Observed identifier, but "Open With…" is an interactive chooser with
        // no non-interactive execution, so it must not be offered (SPEC §5.7).
        assert!(!known_actions().contains(&"alfred.action.openwith"));
    }

    #[test]
    fn saveassnippet_derives_a_usable_keyword() {
        // SPEC §5.5: a saved snippet must carry a keyword that can actually
        // fire; `match_expand_rows` ignores keywords shorter than 2 characters.
        let _guard = crate::tests::env_lock();
        let dst = std::env::temp_dir().join(format!("ralfred_univ_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dst);
        std::fs::create_dir_all(&dst).unwrap();
        let prev = std::env::var("RALFRED_HOME").ok();
        std::env::set_var("RALFRED_HOME", &dst);
        let res = run_action(
            "alfred.action.text.saveassnippet",
            "mailto someone@example.com",
        );
        let saved = crate::db::Store::snippets()
            .all_snippets()
            .into_iter()
            .find(|s| s.text.contains("someone@example.com"));
        match prev {
            Some(p) => std::env::set_var("RALFRED_HOME", p),
            None => std::env::remove_var("RALFRED_HOME"),
        }
        let _ = std::fs::remove_dir_all(&dst);
        assert!(res.is_ok(), "{res:?}");
        let s = saved.expect("snippet stored");
        assert!(
            s.keyword.chars().count() >= 2,
            "keyword {:?} can never expand",
            s.keyword
        );
    }

    #[test]
    fn saveassnippet_rejects_underivable_keyword() {
        let e = run_action("alfred.action.text.saveassnippet", "!!!").unwrap_err();
        assert!(e.to_string().contains("keyword"), "{e}");
    }
}
