//! Icon rendering. Evidence: "Adding icon to cache type %ld key '%@'",
//! iconCacheUpdated:, tidyItemIconCache, icon.png in the binary — Alfred
//! renders per-item icons into a persistent cache keyed by item.
//!
//! Clean-room: resolve app -> bundled .icns, rasterize to 64px PNG with
//! `sips` (no QuickLook daemon involvement — qlmanage wedges here), cache by
//! path hash. Same cache dir powers both the GUI <img> and the CLI.

use std::path::{Path, PathBuf};
use std::process::Command;

fn cache_dir() -> PathBuf {
    let p = crate::config::data_dir().join("iconcache");
    let _ = std::fs::create_dir_all(&p);
    p
}

fn key_for(p: &Path) -> String {
    // cheap path hash (fnv1a) — no crypto needed for a cache key
    let mut h: u64 = 0xcbf29ce484222325;
    for b in p.to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

/// Find the .icns backing a path: bundle (prefer AppIcon*), direct icns, or
/// a sibling .icns next to the item.
pub fn icns_for(p: &Path) -> Option<PathBuf> {
    if p.is_file() {
        if p.extension().and_then(|e| e.to_str()) == Some("icns") {
            return Some(p.to_path_buf());
        }
        return None;
    }
    if p.is_dir() {
        // app bundle: Contents/Resources/*.icns, prefer AppIcon*
        let res = p.join("Contents/Resources");
        let rd = std::fs::read_dir(&res).ok()?;
        let mut all: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|x| x.extension().and_then(|x| x.to_str()) == Some("icns"))
            .collect();
        all.sort(); // deterministic
        all.sort_by_key(|x| {
            !x.file_stem()
                .map(|s| s.to_string_lossy().contains("AppIcon"))
                .unwrap_or(false)
        });
        all.into_iter().next()
    } else {
        None
    }
}

/// Rasterize `p` to a 64px PNG in the icon cache; returns the cached path.
pub fn icon_png(p: &Path) -> Option<PathBuf> {
    let out = cache_dir().join(format!("{}.png", key_for(p)));
    if out.exists() {
        return Some(out);
    }
    let icns = icns_for(p)?;
    let st = Command::new("/usr/bin/sips")
        .args(["-s", "format", "png", "-Z", "64"])
        .arg(&icns)
        .arg("--out")
        .arg(&out)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if st.success() && out.exists() {
        Some(out)
    } else {
        let _ = std::fs::remove_file(&out);
        None
    }
}

/// CLI: `ralfred icon <path>` — print cached icon path or report.
pub fn cli(args: &[String]) {
    let q = args.join(" ");
    let p = PathBuf::from(q.trim());
    if !p.exists() {
        eprintln!("no such item: {}", q.trim());
        std::process::exit(1);
    }
    match icon_png(&p) {
        Some(c) => println!("{}", c.display()),
        None => {
            eprintln!("no icon found for {}", q.trim());
            std::process::exit(1);
        }
    }
}
