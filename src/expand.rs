//! Snippet auto-expansion. Evidence model: AlfredSnippetExpansionManager +
//! snippets.autoexpand column — type a keyword, hit Space/Return/Tab, the
//! keyword is backspaced and the snippet typed/pasted in its place.
//!
//! Global keystroke watching uses a CGEventTap (Accessibility permission
//! required; failure to create the tap surfaces a clear error). Expansion
//! logic is separated (match_expand/on_key) and unit-tested without the tap.

use crate::db::Store;
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    EventField, CallbackResult,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

const SPACE: u16 = 49;
const RETURN: u16 = 36;
const TAB: u16 = 48;
const DELETE: u16 = 51;

/// US-layout key code -> char. Codes from Carbon Events.h (kVK_ANSI_*),
/// verified against the local SDK header — not from memory.
pub fn us_char(code: u16, shift: bool) -> Option<char> {
    let (a, b): (char, char) = match code {
        0 => ('a', 'A'),
        1 => ('s', 'S'),
        2 => ('d', 'D'),
        3 => ('f', 'F'),
        4 => ('h', 'H'),
        5 => ('g', 'G'),
        6 => ('z', 'Z'),
        7 => ('x', 'X'),
        8 => ('c', 'C'),
        9 => ('v', 'V'),
        11 => ('b', 'B'),
        12 => ('q', 'Q'),
        13 => ('w', 'W'),
        14 => ('e', 'E'),
        15 => ('r', 'R'),
        16 => ('y', 'Y'),
        17 => ('t', 'T'),
        18 => ('1', '!'),
        19 => ('2', '@'),
        20 => ('3', '#'),
        21 => ('4', '$'),
        22 => ('6', '^'),
        23 => ('5', '%'),
        24 => ('=', '+'),
        25 => ('9', '('),
        26 => ('7', '&'),
        27 => ('-', '_'),
        28 => ('8', '*'),
        29 => ('0', ')'),
        30 => (']', '}'),
        31 => ('o', 'O'),
        32 => ('u', 'U'),
        33 => ('[', '{'),
        34 => ('i', 'I'),
        35 => ('p', 'P'),
        37 => ('l', 'L'),
        38 => ('j', 'J'),
        39 => ('\'', '"'),
        40 => ('k', 'K'),
        41 => (';', ':'),
        42 => ('\\', '|'),
        43 => (',', '<'),
        44 => ('/', '?'),
        45 => ('n', 'N'),
        46 => ('m', 'M'),
        47 => ('.', '>'),
        49 => (' ', ' '),
        _ => return None,
    };
    Some(if shift { b } else { a })
}

/// Does the buffer's tail match a keyword ending exactly at buffer end,
/// with a word boundary before it? Pure: takes (keyword, snippet) rows,
/// unit-testable without DB or event tap.
pub fn match_expand_rows(buf: &str, rows: &[(String, String)]) -> Option<(String, String)> {
    for (kw, sn) in rows {
        if kw.len() < 2 || buf.len() < kw.len() {
            continue;
        }
        let t = buf.len() - kw.len();
        if !buf[t..].eq_ignore_ascii_case(kw) {
            continue;
        }
        if t > 0 {
            let before = buf.as_bytes()[t - 1];
            if before.is_ascii_alphanumeric() || before == b'_' {
                continue;
            }
        }
        return Some((kw.clone(), sn.clone()));
    }
    None
}

/// Store-backed convenience used by Watcher.
pub fn match_expand(buf: &str, store: &Store) -> Option<(String, String)> {
    match_expand_rows(buf, &store.autoexpand_snippets())
}

pub struct Watcher {
    store: Store,
    pub rows: Vec<(String, String)>,
    /// when true, on_key decides but never posts synthetic key events —
    /// used by unit tests so they cannot touch the real session.
    pub simulate: bool,
    pub buffer: String,
    cfg: crate::config::Config,
}

impl Watcher {
    pub fn new() -> Self {
        let mut w = Self {
            store: Store::snippets(),
            rows: Vec::new(),
            simulate: false,
            buffer: String::new(),
            cfg: crate::config::Config::load(),
        };
        w.reload();
        w
    }

    pub fn reload(&mut self) {
        self.rows = self.store.autoexpand_snippets();
    }

    /// One key event. Trigger keys attempt expansion. Returns Some(keyword)
    /// when an expansion fired.
    pub fn on_key(&mut self, code: u16, shift: bool) -> Option<String> {
        if !self.cfg.auto_expand_snippets {
            return None;
        }
        match code {
            SPACE | RETURN | TAB => {
                let (kw, sn) = match_expand_rows(&self.buffer, &self.rows)?;
                if !self.simulate {
                    self.erase_and_type(&kw, &sn);
                }
                self.buffer.clear();
                Some(kw)
            }
            DELETE => {
                self.buffer.pop();
                None
            }
            c => {
                if c == 49 {
                    self.buffer.push(' ');
                } else if let Some(ch) = us_char(c, shift) {
                    self.buffer.push(ch);
                }
                if self.buffer.len() > 256 {
                    let cut = self.buffer.len() - 256;
                    self.buffer.drain(..cut);
                }
                None
            }
        }
    }

    fn erase_and_type(&self, kw: &str, sn: &str) {
        let src = match CGEventSource::new(CGEventSourceStateID::CombinedSessionState) {
            Ok(s) => s,
            Err(_) => return,
        };
        // 1) backspace the keyword away
        for _ in 0..kw.len() {
            if let (Ok(d), Ok(u)) = (
                CGEvent::new_keyboard_event(src.clone(), DELETE, true),
                CGEvent::new_keyboard_event(src.clone(), DELETE, false),
            ) {
                d.post(CGEventTapLocation::Session);
                u.post(CGEventTapLocation::Session);
            }
        }
        // 2) paste snippet via clipboard swap + Cmd-V
        let ok = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut p| {
                use std::io::Write;
                if let Some(mut si) = p.stdin.take() {
                    let _ = si.write_all(sn.as_bytes());
                }
                p.wait().map(|_| ())
            })
            .is_ok();
        if ok {
            if let (Ok(d), Ok(u)) = (
                CGEvent::new_keyboard_event(src.clone(), 9, true),
                CGEvent::new_keyboard_event(src.clone(), 9, false),
            ) {
                use core_graphics::event::CGEventFlags;
                d.set_flags(CGEventFlags::CGEventFlagCommand);
                u.set_flags(CGEventFlags::CGEventFlagCommand);
                d.post(CGEventTapLocation::Session);
                u.post(CGEventTapLocation::Session);
            }
        }
    }
}

/// Foreground watcher: installs the tap and blocks. Requires Accessibility.
/// with_enabled keeps the tap on this thread's run loop (tap requires the
/// thread run loop to run — documented core-graphics contract).
pub fn serve() -> Result<(), String> {
    let w = std::sync::Arc::new(std::sync::Mutex::new(Watcher::new()));
    core_graphics::event::CGEventTap::with_enabled(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::KeyDown],
        move |_p, _ty, ev| {
            let code = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let shift = ev.get_flags().contains(core_graphics::event::CGEventFlags::CGEventFlagShift);
            let w2 = w.clone();
            if let Ok(mut w) = w2.lock() {
                w.on_key(code, shift);
            }
            CallbackResult::Keep
        },
        || {
            // pump run loop forever: tap callbacks dispatch from here
            extern "C" {
                fn CFRunLoopRun();
            }
            unsafe { CFRunLoopRun() };
        },
    )
    .map_err(|_| "cannot install event tap — grant Accessibility (System Settings > Privacy & Security)".to_string())
}

/// Print which autoexpand snippets are active (dry info command).
pub fn list() {
    let store = Store::snippets();
    let rows = store.autoexpand_snippets();
    if rows.is_empty() {
        println!("no autoexpand snippets (autoexpand=1) — set a snippet autoexpand to enable");
    }
    for (kw, _) in rows {
        println!("autoexpand: {}", kw);
    }
}
