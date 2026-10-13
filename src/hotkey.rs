//! Global hotkey — registered via global-hotkey (Carbon RegisterEventHotKey).
//! macOS requires a running event loop; `ralfred hotkey` provides one.
//! Trigger records "hotkey" history so `ralfred hotkey list` can rank it.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use std::str::FromStr;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};

pub fn parse_combo(s: &str) -> Result<HotKey, String> {
    // "CTRL+SPACE", "ALT+CTRL+K", "SHIFT+/" — modifiers case-insensitive
    let parts: Vec<&str> = s.split('+').collect();
    if parts.len() < 2 {
        return Err(format!("need modifier+key, got: {s}"));
    }
    let (key_part, mod_parts) = (parts[parts.len() - 1], &parts[..parts.len() - 1]);
    let mut mods = Modifiers::empty();
    for p in mod_parts {
        match p.trim().to_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" | "opt" => mods |= Modifiers::ALT,
            "cmd" | "command" | "super" => mods |= Modifiers::META,
            "shift" => mods |= Modifiers::SHIFT,
            other => return Err(format!("unknown modifier: {other}")),
        }
    }
    Ok(HotKey::new(Some(mods), parse_code(key_part)?))
}

/// Map user key spelling onto keyboard_types canonical names.
fn parse_code(key: &str) -> Result<Code, String> {
    let k = key.to_lowercase();
    let name = match k.as_str() {
        "space" => "Space",
        "enter" | "return" => "Enter",
        "tab" => "Tab",
        "esc" | "escape" => "Escape",
        other if other.len() == 1 && other.chars().next().unwrap().is_ascii_alphabetic() => {
            return Code::from_str(&format!("Key{}", other.to_uppercase()))
                .map_err(|_| "bad key".to_string());
        }
        other if other.len() == 1 && other.chars().next().unwrap().is_ascii_digit() => {
            return Code::from_str(&format!("Digit{other}")).map_err(|_| "bad digit".to_string());
        }
        "slash" => "Slash",
        "minus" => "Minus",
        "equal" | "equals" => "Equal",
        other => other,
    };
    Code::from_str(name).map_err(|_| format!("unsupported key: {key}"))
}

/// Registers the combo and blocks until Ctrl-C / SIGTERM.
/// Returns after the OS fires the hotkey — we log the trigger count each round.
pub fn serve(combo: &str) -> Result<(), String> {
    let combo = combo.to_string();
    let hk = parse_combo(&combo)?;
    let manager = GlobalHotKeyManager::new().map_err(|e| format!("manager: {e}"))?;
    manager.register(hk).map_err(|e| format!("register: {e} (accessibility/permissions? already taken?)"))?;
    let events = GlobalHotKeyEvent::receiver();
    eprintln!("ralfred hotkey: {combo} registered (Ctrl-C to quit)");
    let id = hk.id;
    // events channel is global; a worker thread drains it while the main
    // thread pumps the CFRunLoop that delivers the taps (macOS requirement).
    std::thread::spawn(move || loop {
        match events.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(ev) => {
                if ev.id() == id {
                    let store = crate::db::Store::knowledge();
                    store.record_use("hotkey", &format!("HK:{combo}"));
                    eprintln!("triggered {combo}");
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    });
    run_loop();
    Ok(())
}

/// CFRunLoopRun — the OS dispatches hotkey taps only while the main run
/// loop pumps (global-hotkey installs into CFRunLoopGetMain).
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRunLoopRun();
}

fn run_loop() {
    unsafe { CFRunLoopRun() }
}

pub fn list() {
    let store = crate::db::Store::knowledge();
    let rows = store.last_used_prefix("HK:");
    if rows.is_empty() {
        println!("no hotkey activations recorded yet");
    }
    for (item, ts) in rows {
        let h = ((crate::db::now_ts() - ts) / 3600.0).max(0.0) as u64;
        println!("{}\tlast {}h ago", item, h);
    }
}
