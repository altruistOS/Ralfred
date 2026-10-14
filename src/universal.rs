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
            let store = crate::db::Store::snippets();
            let uid = store.add_snippet("", &t, "");
            println!("saved snippet {}", uid);
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
        _ => Ok(()),
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
        "alfred.action.copyfile" => "Copy File",
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
        "alfred.action.openwith",
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
}
