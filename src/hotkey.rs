//! Global hotkey — registered via global-hotkey (Carbon RegisterEventHotKey).
//! macOS requires a running event loop; `ralfred hotkey` provides one.
//! Trigger records "hotkey" history so `ralfred hotkey list` can rank it.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use std::str::FromStr;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};

pub fn parse_combo(s: &str) -> Result<HotKey, String> {
    // "CTRL+SPACE", "ALT+CTRL+K", "SHIFT+/" — modifiers case-insensitive
    // defensive trim: stray trailing junk from the command line (e.g. an
    // accidental `"/` closing the user's quote) should not break registration.
    // But a trailing `/` that is itself the key (`CTRL+/`) must survive — so
    // strip it only when it hangs off a longer key token, not off a `+`.
    let mut s = s.trim();
    while s.ends_with('/') {
        let before = &s[..s.len() - 1];
        s = if before.ends_with('+') { break; } else { before };
    }
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
        "slash" | "/" => "Slash",
        "minus" | "-" => "Minus",
        "equal" | "equals" | "=" => "Equal",
        other => other,
    };
    Code::from_str(name).map_err(|_| format!("unsupported key: {key}"))
}

/// Registers the combo and blocks until Ctrl-C / SIGTERM.
/// Returns after the OS fires the hotkey — we log the trigger count each round.

// ---------------------------------------------------------------------------
// CGEventTap hotkey watcher (belt-and-braces over Carbon RegisterEventHotKey).
// The Carbon path is unreliable for plain-option combos on recent macOS and
// dies silently after re-registration churn; a session tap sees every keydown
// regardless of which app is focused, which is exactly the Alfred guarantee
// ("wake anywhere"). Needs Accessibility — same permission autoexpand uses.
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicU64, Ordering as MemOrder};

/// ms of the most recent hotkey delivery — both dispatch paths stamp this so
/// the panel never receives the same summon twice.
pub static LAST_HOTKEY_MS: AtomicU64 = AtomicU64::new(0);

pub fn stamp_and_take(window_ms: u64) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let prev = LAST_HOTKEY_MS.swap(now, MemOrder::Relaxed);
    now.saturating_sub(prev) >= window_ms
}

fn code_to_raw(code: &Code) -> Option<u16> {
    let c = match code {
        Code::Space => 49,
        Code::Enter => 36,
        Code::Tab => 48,
        Code::Escape => 53,
        Code::Slash => 44,
        Code::Minus => 27,
        Code::Equal => 24,
        Code::Backspace => 51,
        Code::KeyA => 0, Code::KeyB => 11, Code::KeyC => 8, Code::KeyD => 2, Code::KeyE => 14,
        Code::KeyF => 3, Code::KeyG => 5, Code::KeyH => 4, Code::KeyI => 34, Code::KeyJ => 38,
        Code::KeyK => 40, Code::KeyL => 37, Code::KeyM => 46, Code::KeyN => 45, Code::KeyO => 31,
        Code::KeyP => 35, Code::KeyQ => 12, Code::KeyR => 15, Code::KeyS => 1, Code::KeyT => 17,
        Code::KeyU => 32, Code::KeyV => 9, Code::KeyW => 13, Code::KeyX => 7, Code::KeyY => 16,
        Code::KeyZ => 6,
        Code::Digit1 => 18, Code::Digit2 => 19, Code::Digit3 => 20, Code::Digit4 => 21,
        Code::Digit5 => 23, Code::Digit6 => 22, Code::Digit7 => 26, Code::Digit8 => 28,
        Code::Digit9 => 25, Code::Digit0 => 29,
        _ => return None,
    };
    Some(c)
}

/// Required CGEventFlags for the combo's modifiers; None if unsupported.
fn required_flags(mods: Modifiers) -> Option<u64> {
    let mut f: u64 = 0;
    if mods.contains(Modifiers::SHIFT) { f |= 0x20000; }
    if mods.contains(Modifiers::CONTROL) { f |= 0x10000; }
    if mods.contains(Modifiers::ALT) { f |= 0x80000; }
    if mods.contains(Modifiers::META) { f |= 0x100000; }
    if f == 0 { None } else { Some(f) }
}

/// Install a listen-only session tap that fires `fired()` on every press of
/// the currently-configured combo. The combo is re-read on every event so
/// settings changes take effect immediately.
pub fn start_tap_watcher<F: Fn() + Send + 'static>(fired: F) -> Result<(), String> {
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        CallbackResult, EventField,
    };
    let slot = std::sync::Arc::new(std::sync::Mutex::new((u16::MAX, 1u64 << 40, String::new())));
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let counter_c = counter.clone();
    std::thread::spawn(move || {
        let w = slot.clone();
        
        let res = CGEventTap::with_enabled(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![CGEventType::KeyDown],
            move |_p, _ty, ev| {
                let counter = counter_c.clone();
                let cur = crate::config::Config::load().hotkey;
                // (see counter log below)
                let (code, flags) = {
                    let mut g = w.lock().unwrap();
                    if g.2 != cur {
                        if let Ok(hk) = parse_combo(&cur) {
                            g.0 = code_to_raw(&hk.key).unwrap_or(u16::MAX);
                            g.1 = required_flags(hk.mods).unwrap_or(1u64 << 40);
                            g.2 = cur.clone();
                        }
                    }
                    (g.0, g.1)
                };
                let kc = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n <= 3 {
                    eprintln!("[hk tap] event #{} kc={} want={} flags={:x}", n, kc, code, ev.get_flags().bits());
                }
                if kc == code {
                    let mod_bits = 0x1B_0000u64; // ctrl|shift|opt|cmd mask
                    let fl = ev.get_flags().bits() & mod_bits;
                    if fl == flags && stamp_and_take(500) {
                        fired();
                    }
                }
                CallbackResult::Keep
            },
            || {
                extern "C" {
                    fn CFRunLoopRun();
                }
                unsafe { CFRunLoopRun() };
            },
        );
        if res.is_err() {
            eprintln!("[hk] tap unavailable (grant Accessibility) — relying on Carbon path");
        }
    });
    Ok(())
}

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
