//! Install / autostart. Alfred's equivalent is its LoginItems helper; we use
//! the conventional macOS path: a LaunchAgent plist in ~/Library/LaunchAgents
//! plus `launchctl bootstrap/bootout`. `ralfred install [--hotkey ALT+CTRL+SPACE]`
//! registers the GUI as a login item; `uninstall` removes it; `status` reports.
//!
//! Test hook: RALFRED_INSTALL_NOLAUNCHD=1 skips the launchctl calls but keeps
//! the full plist write/unlink lifecycle (so e2e can verify file state without
//! actually spawning GUI panels on the desktop).

use std::path::PathBuf;
use std::process::Command;

/// getuid via libc's exported symbol (libc is linked transitively; no crate dep)
fn uid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

pub const LABEL: &str = "com.kleo.ralfred";

fn plist_path() -> PathBuf {
    if let Some(root) = SANDBOX_ROOT.get() {
        return root
            .join("LaunchAgents")
            .join(format!("{}.plist", LABEL));
    }
    if let Ok(home) = std::env::var("RALFRED_HOME") {
        return PathBuf::from(home)
            .join("LaunchAgents")
            .join(format!("{}.plist", LABEL));
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", LABEL))
}

static SANDBOX_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Test hook: pin plist location to a sandbox dir (no env racing with other
/// tests). Set once per process, before install/uninstall/status.
pub fn use_sandbox(root: &std::path::Path) {
    let _ = SANDBOX_ROOT.set(root.to_path_buf());
}

pub fn plist_xml(hotkey: &str, binary: &str, log_dir: Option<&str>) -> String {
    let dir = log_dir.map(|d| format!("    <key>StandardOutPath</key>\n    <string>{}/ralfred.log</string>\n    <key>StandardErrorPath</key>\n    <string>{}/ralfred.err</string>\n", d, d)).unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>gui</string>
        <string>{hotkey}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
{dir}    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#
    )
}

fn launchctl(action: &str) -> bool {
    let uid = uid();
    Command::new("launchctl")
        .args([action, &format!("gui/{uid}"), &plist_path().to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn install(hotkey: &str) -> Result<(), String> {
    let binary = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "ralfred".into());
    let log_dir = crate::config::data_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let p = plist_path();
    if let Some(par) = p.parent() {
        std::fs::create_dir_all(par).map_err(|e| format!("mkdir: {e}"))?;
    }
    std::fs::write(&p, plist_xml(hotkey, &binary, Some(&log_dir.to_string_lossy())))
        .map_err(|e| format!("write plist: {e}"))?;
    if std::env::var("RALFRED_INSTALL_NOLAUNCHD").is_ok() {
        return Ok(());
    }
    launchctl("bootout"); // idempotent if not loaded
    if launchctl("bootstrap") {
        Ok(())
    } else {
        // bootstrap failed: leave plist but be honest about it
        Err("plist written but launchctl bootstrap failed (already loaded or GUI session?)".into())
    }
}

pub fn uninstall() -> Result<(), String> {
    if !plist_path().exists() {
        return Ok(()); // idempotent
    }
    if std::env::var("RALFRED_INSTALL_NOLAUNCHD").is_err() {
        launchctl("bootout");
    }
    std::fs::remove_file(plist_path()).map_err(|e| format!("remove plist: {e}"))?;
    Ok(())
}

pub fn status() -> String {
    let p = plist_path();
    if !p.exists() {
        return "not installed".into();
    }
    if std::env::var("RALFRED_INSTALL_NOLAUNCHD").is_ok() {
        return format!("installed (plist) at {}", p.display());
    }
    let uid = uid();
    let loaded = Command::new("launchctl")
        .args(["print", &format!("gui/{uid}/{LABEL}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if loaded {
        format!("installed & loaded ({})", p.display())
    } else {
        format!("installed, not loaded ({})", p.display())
    }
}

pub fn cli(args: &[String]) {
    let verb = args.first().map(|s| s.as_str());
    let implicit = verb.map(|v| v.starts_with("--")).unwrap_or(true);
    match if implicit { Some("install") } else { verb } {
        Some("install") => {
            let hotkey = args
                .iter()
                .position(|a| a == "--hotkey")
                .and_then(|i| args.get(i + 1).cloned())
                .unwrap_or_else(|| "ALT+CTRL+SPACE".into());
            match install(&hotkey) {
                Ok(()) => println!("installed: login item {LABEL} (hotkey {hotkey})"),
                Err(e) => {
                    eprintln!("install: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some("uninstall") => match uninstall() {
            Ok(()) => println!("uninstalled {LABEL}"),
            Err(e) => {
                eprintln!("uninstall: {e}");
                std::process::exit(1);
            }
        },
        Some("status") => println!("{}", status()),
        _ => {
            eprintln!("usage: ralfred install [--hotkey ALT+CTRL+SPACE] | uninstall | status");
            std::process::exit(1);
        }
    }
}
