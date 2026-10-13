//! Quick Look. Evidence: toggleQuickLook, generateQuicklookURL,
//! quicklookURL, quickLookTemporaryFolder, "Quicklooking '%@'" and
//! "Deleteing Clipboard QuickLook folder" in the binary — space toggles a
//! qlmanage preview, and non-file items (clipboard text) are written to a
//! temporary folder and file://-ed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::OnceLock;

static PREVIEW: OnceLock<Mutex<Option<(u32, String)>>> = OnceLock::new();

fn preview_state() -> &'static Mutex<Option<(u32, String)>> {
    PREVIEW.get_or_init(|| Mutex::new(None))
}

fn tmp_folder() -> PathBuf {
    let mut p = crate::config::data_dir();
    p.push("quicklook");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// URL previewed for an item: real path as file://, or text content
/// materialized into the temporary folder (Alfred's clipboard-QuickLook trick).
pub fn preview_url(item_path: Option<&str>, text: Option<&str>) -> Option<String> {
    if let Some(p) = item_path {
        let pb = Path::new(p);
        if pb.exists() {
            return Some(format!(
                "file://{}",
                pb.canonicalize().unwrap_or_else(|_| pb.to_path_buf()).display()
            ));
        }
    }
    if let Some(t) = text {
        let f = tmp_folder().join("ql.txt");
        if std::fs::write(&f, t).is_ok() {
            return Some(format!("file://{}", f.display()));
        }
    }
    None
}

/// True if the system Quick Look helper exists.
pub fn available() -> bool {
    Path::new("/usr/bin/qlmanage").exists()
}

fn close_current() {
    if let Some((pid, _)) = preview_state().lock().unwrap().take() {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    // best-effort: dismiss the shared QuickLook panel (qlmanage's panel lives
    // in a qlgenerator helper; this is the documented dismissal trick)
    let _ = Command::new("killall")
        .arg("qlmanage")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Toggle: same URL -> close preview; new/none -> open. Returns Some(url)
/// opened or None when closed.
pub fn toggle(url: &str) -> Option<String> {
    if std::env::var("RALFRED_QL_NOEXEC").is_ok() {
        // test mode: drive the state machine without launching the panel
        let cur = preview_state().lock().unwrap().take();
        if let Some((_, cur_url)) = cur {
            if cur_url == url {
                if let Some(p) = cur_url.strip_prefix("file://") {
                    if std::path::Path::new(p).parent() == Some(&tmp_folder()) {
                        let _ = std::fs::remove_file(p);
                    }
                }
                return None;
            }
        }
        *preview_state().lock().unwrap() = Some((std::process::id(), url.to_string()));
        return Some(url.to_string());
    }
    {
        let cur = preview_state().lock().unwrap().take();
        if let Some((_, cur_url)) = cur {
            if cur_url == url {
                // same item: second press closes
                if let Some(p) = cur_url.strip_prefix("file://") {
                    if std::path::Path::new(p).parent() == Some(&tmp_folder()) {
                        let _ = std::fs::remove_file(p); // clipboard temp cleanup
                    }
                }
                close_current();
                return None;
            }
            // switching previews: close old, open new below
            close_current();
        }
    }
    let child = Command::new("/usr/bin/qlmanage")
        .arg("-p")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match child {
        Ok(mut c) => {
            let pid = c.id();
            // qlmanage returns quickly; the panel lives in a shared qlgenerator
            let _ = c.try_wait();
            *preview_state().lock().unwrap() = Some((pid, url.to_string()));
            Some(url.to_string())
        }
        Err(_) => None,
    }
}

/// CLI: `ralfred ql <path|text>` — resolve and toggle a preview.
/// With RALFRED_QL_NOEXEC set, only prints the resolved URL (test mode).
pub fn cli(args: &[String]) {
    if args.is_empty() {
        eprintln!("usage: ralfred ql <path>");
        std::process::exit(1);
    }
    if !std::env::var("RALFRED_QL_NOEXEC").is_ok() && !available() {
        eprintln!("qlmanage not found — Quick Look unavailable");
        std::process::exit(1);
    }
    let q = args.join(" ");
    let url = match preview_url(Some(&q), None) {
        Some(u) => u,
        None => match preview_url(None, Some(&q)) {
            Some(u) => u,
            None => {
                eprintln!("nothing to preview");
                std::process::exit(1);
            }
        },
    };
    if std::env::var("RALFRED_QL_NOEXEC").is_ok() {
        println!("quicklook: {url}");
        return;
    }
    match toggle(&url) {
        Some(u) => println!("quicklook: {u}"),
        None => println!("quicklook: closed"),
    }
}
