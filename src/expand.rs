//! Snippet auto-expansion. Evidence model: AlfredSnippetExpansionManager +
//! snippets.autoexpand column — type a keyword, hit Space/Return/Tab, the
//! keyword is backspaced and the snippet typed/pasted in its place.
//!
//! Global keystroke watching uses a CGEventTap (Accessibility permission
//! required; failure to create the tap surfaces a clear error). Expansion
//! logic is separated (match_expand/on_key) and unit-tested without the tap.

use crate::db::Store;
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType, EventField,
    CallbackResult,
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
        if ends_with_keyword(buf, kw) {
            return Some((kw.clone(), sn.clone()));
        }
    }
    None
}

/// As [`match_expand_rows`], but carries each row's `ignoredynamicplaceholders`
/// flag so the caller can skip placeholder expansion (SPEC §5.5).
pub fn match_expand_rows_full(
    buf: &str,
    rows: &[(String, String, bool)],
) -> Option<(String, String, bool)> {
    for (kw, sn, ignored) in rows {
        if ends_with_keyword(buf, kw) {
            return Some((kw.clone(), sn.clone(), *ignored));
        }
    }
    None
}

/// True when `buf` ends with `kw` at a word boundary. Uses `str::get` so a
/// keyword match can never slice `buf` at a non-UTF-8 boundary.
fn ends_with_keyword(buf: &str, kw: &str) -> bool {
    if kw.len() < 2 || buf.len() < kw.len() {
        return false;
    }
    let t = buf.len() - kw.len();
    let Some(tail) = buf.get(t..) else {
        return false;
    };
    if !tail.eq_ignore_ascii_case(kw) {
        return false;
    }
    if t > 0 {
        let before = buf.as_bytes()[t - 1];
        if before.is_ascii_alphanumeric() || before == b'_' {
            return false;
        }
    }
    true
}

/// Store-backed convenience used by Watcher.
pub fn match_expand(buf: &str, store: &Store) -> Option<(String, String)> {
    match_expand_rows(buf, &store.autoexpand_snippets())
}

pub struct Watcher {
    store: Store,
    pub rows: Vec<(String, String, bool)>,
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
        self.rows = self.store.autoexpand_snippets_full();
    }

    /// One key event. Trigger keys attempt expansion. Returns Some(keyword)
    /// when an expansion fired.
    pub fn on_key(&mut self, code: u16, shift: bool) -> Option<String> {
        if !self.cfg.auto_expand_snippets {
            return None;
        }
        match code {
            SPACE | RETURN | TAB => {
                let (kw, sn, ignored) = match_expand_rows_full(&self.buffer, &self.rows)?;
                if !self.simulate {
                    self.erase_and_type(&kw, &sn, ignored);
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

    fn erase_and_type(&self, kw: &str, sn: &str, ignored_placeholders: bool) {
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
        // 2) paste snippet via clipboard swap + Cmd-V. SPEC §5.5: dynamic
        // placeholders are expanded at insertion time unless the snippet opted
        // out with `ignoredynamicplaceholders`. A `{cursor}` marker is consumed
        // by the expander; this path pastes text, so it cannot position the
        // caret afterwards — the marker is removed either way.
        let expanded = if ignored_placeholders {
            sn.to_string()
        } else {
            expand_placeholders_default(sn)
        };
        let body = expanded.as_bytes();
        let ok = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut p| {
                use std::io::Write;
                if let Some(mut si) = p.stdin.take() {
                    let _ = si.write_all(body);
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

// ---------------------------------------------------------------------------
// Dynamic snippet placeholders (SPEC §5.5 and §8.11).
//
// The grammar is interface evidence from the Alfred 5.8 binary; how it is
// evaluated here is this project's own choice, recorded where it is not forced
// by the grammar (date styles and defaults in particular).
//
// Wired: `Watcher::erase_and_type` expands placeholders at insertion time and
// skips expansion for rows whose `ignoredynamicplaceholders` flag is set (read
// through `Store::autoexpand_snippets_full`). `{cursor}` is consumed by the
// expander; insertion goes through a paste, so the caret cannot be positioned
// afterwards and only the marker's removal takes effect on that path.
// ---------------------------------------------------------------------------

/// Maximum nesting depth for `{snippet:<keyword>}` expansion. A snippet whose
/// body contains its own keyword stops at this bound (SPEC §8.11); the marker
/// then stays literal rather than expanding forever.
pub const MAX_PLACEHOLDER_DEPTH: usize = 4;

/// Externally supplied inputs for `expand_placeholders`. Everything a
/// placeholder can observe is injected here, so expansion is deterministic in
/// tests: no clipboard store, wall clock or entropy is consulted unless the
/// caller supplies it.
pub struct PlaceholderCtx {
    /// Clipboard history, most recent first. Index 0 is the current clipboard,
    /// matching `{clipboard}`, `{clipboard:0}` and `{clipboard:trim}`.
    pub clipboard: Vec<String>,
    /// Unix epoch seconds that `{date}`, `{datetime}`, `{time}` and
    /// `{isodatetime}` are rendered at.
    pub now_unix: i64,
    /// UTC offset in seconds applied when breaking `now_unix` into civil time.
    pub utc_offset_secs: i64,
    /// `{random:UUID}` generator.
    pub uuid: Box<dyn Fn() -> String>,
    /// `{random:...}` entropy source, returning a unit value in `[0, 1)`.
    pub rng: Box<dyn Fn() -> f64>,
    /// Body lookup for `{snippet:<keyword>}`; the returned text is expanded
    /// again under [`MAX_PLACEHOLDER_DEPTH`]. `None` leaves the marker verbatim.
    pub snippet: Option<Box<dyn Fn(&str) -> Option<String>>>,
}

impl Default for PlaceholderCtx {
    /// Live context: real clock (in local time), real entropy, real UUIDs and
    /// no clipboard or snippet lookups. Use [`PlaceholderCtx::from_store`] to
    /// bind the on-disk stores.
    fn default() -> Self {
        let now = now_epoch_secs();
        Self {
            clipboard: Vec::new(),
            now_unix: now,
            utc_offset_secs: local_offset_secs(now),
            uuid: Box::new(crate::db::uuid_v4),
            rng: Box::new(rand_unit),
            snippet: None,
        }
    }
}

impl PlaceholderCtx {
    /// Deterministic context for tests: no clipboard history, time pinned to
    /// 1970-01-01T00:00:00Z, zero entropy and a fixed UUID.
    pub fn test() -> Self {
        Self {
            clipboard: Vec::new(),
            now_unix: 0,
            utc_offset_secs: 0,
            uuid: Box::new(|| "00000000-0000-4000-8000-000000000000".to_string()),
            rng: Box::new(|| 0.0),
            snippet: None,
        }
    }

    /// Context bound to the live stores (`clipboard.alfdb`, `snippets.alfdb`)
    /// and the wall clock. A store that cannot be opened degrades to an empty
    /// input instead of panicking the expansion path (SPEC §9.3).
    pub fn from_store() -> Self {
        let mut ctx = Self::default();
        ctx.clipboard = std::panic::catch_unwind(|| Store::clipboard().clipboard_recent(200))
            .map(|rows| rows.into_iter().map(|(item, _)| item).collect())
            .unwrap_or_default();
        ctx.snippet = Some(Box::new(|kw: &str| {
            let kw = kw.to_string();
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                Store::snippets()
                    .find_snippets(&kw)
                    .into_iter()
                    .find(|s| s.keyword == kw)
                    .map(|s| s.text)
            }))
            .ok()
            .flatten()
        }));
        ctx
    }
}

/// Expand every dynamic placeholder in `text`.
///
/// Returns the expanded text and, when the input contained a `{cursor}`
/// marker, the marker's position as a **character** index into the expanded
/// text (the marker itself is removed). `None` means "no caret request".
/// Unknown or malformed `{...}` tokens are left verbatim — they are not
/// errors (SPEC §5.5).
pub fn expand_placeholders(text: &str, ctx: &PlaceholderCtx) -> (String, Option<usize>) {
    expand_at_depth(text, ctx, 0)
}

/// Convenience for the production path: live stores, clock and entropy, caret
/// offset discarded.
pub fn expand_placeholders_default(text: &str) -> String {
    expand_placeholders(text, &PlaceholderCtx::from_store()).0
}

/// One expansion pass. `depth` counts nested `{snippet:...}` expansions.
fn expand_at_depth(text: &str, ctx: &PlaceholderCtx, depth: usize) -> (String, Option<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut out_chars = 0usize;
    let mut cursor: Option<usize> = None;
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        push(&mut out, &mut out_chars, &rest[..open]);
        let after_open = &rest[open + 1..];
        let close = match after_open.find('}') {
            Some(i) => i,
            None => {
                // unterminated `{`: the remainder is literal, and we are done
                push(&mut out, &mut out_chars, &rest[open..]);
                rest = "";
                break;
            }
        };
        let inner = &after_open[..close];
        let consumed = open + 1 + close + 1;
        if is_cursor_marker(inner) {
            // first marker wins; the text is removed either way
            if cursor.is_none() {
                cursor = Some(out_chars);
            }
        } else if let Some(repl) = expand_inner(inner, ctx, depth) {
            push(&mut out, &mut out_chars, &repl);
        } else {
            push(&mut out, &mut out_chars, &rest[open..consumed]);
        }
        rest = &rest[consumed..];
    }
    push(&mut out, &mut out_chars, rest);
    (out, cursor)
}

fn push(out: &mut String, chars: &mut usize, s: &str) {
    out.push_str(s);
    *chars += s.chars().count();
}

fn is_cursor_marker(inner: &str) -> bool {
    inner.trim().eq_ignore_ascii_case("cursor")
}

/// Replace one placeholder, or `None` to leave it verbatim.
fn expand_inner(inner: &str, ctx: &PlaceholderCtx, depth: usize) -> Option<String> {
    let t = inner.trim();
    if let Some(rest) = strip_prefix_ci(t, "clipboard") {
        if rest.is_empty() || rest.starts_with(':') || rest.starts_with('.') {
            return expand_clipboard(rest, ctx);
        }
    }
    if t.eq_ignore_ascii_case("isodatetime") {
        let c = Civil::from_epoch(ctx.now_unix, ctx.utc_offset_secs);
        return Some(format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            c.year, c.month, c.day, c.hour, c.minute, c.second
        ));
    }
    // longest keyword first: "datetime" before "date"/"time"
    for (kw, kind) in [("datetime", Kind::DateTime), ("date", Kind::Date), ("time", Kind::Time)] {
        if let Some(rest) = strip_prefix_ci(t, kw) {
            let bare = rest.is_empty()
                || rest.starts_with(':')
                || rest.starts_with(|c: char| c.is_whitespace());
            if bare {
                return date_placeholder(kind, rest, ctx);
            }
        }
    }
    if let Some(spec) = strip_prefix_ci(t, "random:") {
        return expand_random(spec, ctx);
    }
    if let Some(kw) = strip_prefix_ci(t, "snippet:") {
        let lookup = ctx.snippet.as_ref()?;
        let body = lookup(kw.trim())?;
        if depth + 1 >= MAX_PLACEHOLDER_DEPTH {
            // recursion bound reached: insert the body unexpanded (§8.11)
            return Some(body);
        }
        return Some(expand_at_depth(&body, ctx, depth + 1).0);
    }
    None
}

/// Case-insensitive ASCII prefix strip; `None` when `s` is shorter than the
/// prefix or the prefix would end inside a multi-byte character.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.get(..prefix.len())?.eq_ignore_ascii_case(prefix) {
        s.get(prefix.len()..)
    } else {
        None
    }
}

// --- clipboard ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipMod {
    Plain,
    Trim,
    Upper,
    Lower,
    Capitals,
}

/// `arg` is what followed the `clipboard` keyword: "", ":3", ":trim",
/// ":1.trim", ":1:trim", ".lowercase" and so on. The grammar allows `.` or
/// `:` as the separator, so both are accepted.
fn expand_clipboard(arg: &str, ctx: &PlaceholderCtx) -> Option<String> {
    let arg = arg.trim();
    if arg.is_empty() {
        return Some(apply_clip_mod(clipboard_entry(0, ctx), ClipMod::Plain));
    }
    let rest = arg.strip_prefix(':').or_else(|| arg.strip_prefix('.')).unwrap_or(arg);
    if rest.is_empty() {
        // a bare separator, e.g. `{clipboard:}`, is not part of the grammar
        return None;
    }
    let (head, tail) = match rest.find(|c| c == '.' || c == ':') {
        Some(i) => (&rest[..i], Some(&rest[i + 1..])),
        None => (rest, None),
    };
    if head.is_empty() {
        return None;
    }
    if head.bytes().all(|b| b.is_ascii_digit()) {
        let idx: usize = head.parse().ok()?;
        let m = match tail {
            None => ClipMod::Plain,
            Some(t) => clip_mod(t)?,
        };
        Some(apply_clip_mod(clipboard_entry(idx, ctx), m))
    } else {
        // a bare modifier, optionally on the current entry (".trim")
        if tail.is_some() {
            return None;
        }
        Some(apply_clip_mod(clipboard_entry(0, ctx), clip_mod(head)?))
    }
}

/// History entry `idx` (0 = current clipboard). A missing entry expands to the
/// empty string, matching an empty clipboard rather than a literal marker.
fn clipboard_entry(idx: usize, ctx: &PlaceholderCtx) -> String {
    ctx.clipboard.get(idx).cloned().unwrap_or_default()
}

fn clip_mod(s: &str) -> Option<ClipMod> {
    match s.trim().to_ascii_lowercase().as_str() {
        "trim" => Some(ClipMod::Trim),
        "uppercase" => Some(ClipMod::Upper),
        "lowercase" => Some(ClipMod::Lower),
        "capitals" => Some(ClipMod::Capitals),
        _ => None,
    }
}

fn apply_clip_mod(s: String, m: ClipMod) -> String {
    match m {
        ClipMod::Plain => s,
        ClipMod::Trim => s.trim().to_string(),
        ClipMod::Upper => s.to_uppercase(),
        ClipMod::Lower => s.to_lowercase(),
        ClipMod::Capitals => capitals(&s),
    }
}

/// Capitalise the first character of every word, lowercase the rest.
fn capitals(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut boundary = true;
    for c in s.chars() {
        if boundary {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        boundary = !c.is_alphanumeric();
    }
    out
}

// --- date / time ----------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Date,
    DateTime,
    Time,
}

impl Kind {
    /// ralfred choice: Alfred renders these in the user's locale, which is not
    /// available here, so the defaults are fixed and locale-neutral.
    fn default_pattern(self) -> &'static str {
        match self {
            Kind::Date => "yyyy-MM-dd",
            Kind::DateTime => "yyyy-MM-dd HH:mm:ss",
            Kind::Time => "HH:mm:ss",
        }
    }
}

/// Broken-down civil time in an explicit UTC offset. Deliberately not a
/// calendar library: the placeholder grammar needs offsets in years, months,
/// days, hours and minutes, which the standard days-from-civil conversion
/// (proleptic Gregorian) provides exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Civil {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

impl Civil {
    fn from_epoch(t: i64, offset_secs: i64) -> Civil {
        let local = t.saturating_add(offset_secs);
        let days = local.div_euclid(86_400);
        let secs = local.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        Civil {
            year,
            month,
            day,
            hour: (secs / 3600) as u32,
            minute: ((secs % 3600) / 60) as u32,
            second: (secs % 60) as u32,
        }
    }

    fn to_epoch(self, offset_secs: i64) -> Option<i64> {
        let days = days_from_civil(self.year, self.month, self.day);
        days.checked_mul(86_400)?
            .checked_add(self.hour as i64 * 3600 + self.minute as i64 * 60 + self.second as i64)?
            .checked_sub(offset_secs)
    }
}

/// Days since 1970-01-01, proleptic Gregorian (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m as i64 - 3 } else { m as i64 + 9 }; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (y + if m <= 2 { 1 } else { 0 }, m as u32, d as u32)
}

const MONTH_ABBREV: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const MONTH_FULL: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const WEEKDAY_ABBREV: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAY_FULL: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// render `pattern` for a civil time. Supported pattern letters:
/// `EEEE`/`EEE`, `yyyy`/`yy`/`y`, `MMMM`/`MMM`/`MM`/`M`, `dd`/`d`,
/// `HH`/`H`, `mm`/`m`, `ss`/`s`. Any other run of letters — including an
/// unsupported width such as `EE` — is emitted literally.
fn format_civil(c: Civil, pattern: &str) -> String {
    let weekday = weekday_from_days(days_from_civil(c.year, c.month, c.day)) as usize;
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let mut j = i;
        while j < chars.len() && chars[j] == ch {
            j += 1;
        }
        let run = j - i;
        let text: Option<String> = match ch {
            'E' => match run {
                3 => Some(WEEKDAY_ABBREV[weekday].to_string()),
                4 => Some(WEEKDAY_FULL[weekday].to_string()),
                _ => None,
            },
            'y' => match run {
                1 => Some(c.year.to_string()),
                2 => Some(format!("{:02}", c.year.rem_euclid(100))),
                _ => Some(format!("{:04}", c.year)),
            },
            'M' => match run {
                1 => Some(c.month.to_string()),
                2 => Some(format!("{:02}", c.month)),
                3 => Some(MONTH_ABBREV[(c.month - 1) as usize].to_string()),
                4 => Some(MONTH_FULL[(c.month - 1) as usize].to_string()),
                _ => None,
            },
            'd' => match run {
                1 => Some(c.day.to_string()),
                2 => Some(format!("{:02}", c.day)),
                _ => None,
            },
            'H' => match run {
                1 => Some(c.hour.to_string()),
                2 => Some(format!("{:02}", c.hour)),
                _ => None,
            },
            'm' => match run {
                1 => Some(c.minute.to_string()),
                2 => Some(format!("{:02}", c.minute)),
                _ => None,
            },
            's' => match run {
                1 => Some(c.second.to_string()),
                2 => Some(format!("{:02}", c.second)),
                _ => None,
            },
            _ => None,
        };
        match text {
            Some(t) => out.push_str(&t),
            None => out.extend(&chars[i..j]),
        }
        i = j;
    }
    out
}

/// 0 = Sunday, matching `WEEKDAY_*`.
fn weekday_from_days(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32
}

/// Named styles. ralfred choice: the exact strings Alfred emits are
/// locale-dependent and were not established, so these are fixed and
/// documented; datetime combines the date and time forms.
fn named_style(style: &str, kind: Kind) -> Option<String> {
    let (date, time) = match style.to_ascii_lowercase().as_str() {
        "short" => ("yyyy-MM-dd", "HH:mm"),
        "medium" => ("d MMM yyyy", "HH:mm:ss"),
        "long" => ("d MMMM yyyy", "HH:mm:ss"),
        "full" => ("EEEE, d MMMM yyyy", "HH:mm:ss"),
        _ => return None,
    };
    Some(match kind {
        Kind::Date => date.to_string(),
        Kind::Time => time.to_string(),
        Kind::DateTime => format!("{date} {time}"),
    })
}

/// `rest` is what followed the `date`/`datetime`/`time` keyword. The part
/// before the first `:` is the offset list; everything after it is a named
/// style or a pattern (so `{time:HH:mm}` keeps its own colon).
fn date_placeholder(kind: Kind, rest: &str, ctx: &PlaceholderCtx) -> Option<String> {
    let (offset_spec, format_spec) = match rest.find(':') {
        Some(i) => (&rest[..i], Some(&rest[i + 1..])),
        None => (rest, None),
    };
    let (dy, dmo, dd, dh, dmi) = parse_offsets(offset_spec)?;
    let civil = Civil::from_epoch(ctx.now_unix, ctx.utc_offset_secs);
    let civil = apply_offsets(civil, dy, dmo, dd, dh, dmi, ctx.utc_offset_secs)?;
    let pattern = match format_spec {
        None => kind.default_pattern().to_string(),
        Some(f) => {
            let f = f.trim();
            if f.is_empty() {
                kind.default_pattern().to_string()
            } else {
                named_style(f, kind).unwrap_or_else(|| f.to_string())
            }
        }
    };
    Some(format_civil(civil, &pattern))
}

/// Parse `<+|-><number><unit>` tokens separated by whitespace, with unit one of
/// `y M d h m` (minutes). An empty spec means "no offset". Anything malformed
/// returns `None`, so the caller leaves the placeholder verbatim.
fn parse_offsets(spec: &str) -> Option<(i64, i64, i64, i64, i64)> {
    let mut out = (0i64, 0i64, 0i64, 0i64, 0i64);
    for tok in spec.split_whitespace() {
        let mut it = tok.chars();
        let sign: i64 = match it.next() {
            Some('+') => 1,
            Some('-') => -1,
            _ => return None,
        };
        let body: String = it.collect();
        if body.len() < 2 {
            return None;
        }
        let cut = body.len() - 1;
        if !body.is_char_boundary(cut) {
            return None;
        }
        let (num, unit) = body.split_at(cut);
        if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let n: i64 = num.parse().ok()?;
        let v = sign.checked_mul(n)?;
        match unit {
            "y" => out.0 = out.0.checked_add(v)?,
            "M" => out.1 = out.1.checked_add(v)?,
            "d" => out.2 = out.2.checked_add(v)?,
            "h" => out.3 = out.3.checked_add(v)?,
            "m" => out.4 = out.4.checked_add(v)?,
            _ => return None,
        }
    }
    Some(out)
}

/// Apply calendar offsets. Years and months are added to the calendar first
/// (the day-of-month is not clamped: it rolls forward, which is what a pure
/// calendar shift gives), then days/hours/minutes are added as elapsed time.
fn apply_offsets(c: Civil, dy: i64, dmo: i64, dd: i64, dh: i64, dmi: i64, offset_secs: i64) -> Option<Civil> {
    let months = c
        .year
        .checked_mul(12)?
        .checked_add(c.month as i64 - 1)?
        .checked_add(dy.checked_mul(12)?)?
        .checked_add(dmo)?;
    let mut c = c;
    c.year = months.div_euclid(12);
    c.month = (months.rem_euclid(12) + 1) as u32;
    let extra = dd
        .checked_mul(86_400)?
        .checked_add(dh.checked_mul(3_600)?)?
        .checked_add(dmi.checked_mul(60)?)?;
    let t = c.to_epoch(offset_secs)?.checked_add(extra)?;
    Some(Civil::from_epoch(t, offset_secs))
}

// --- random ---------------------------------------------------------------

/// `{random:...}`: an inclusive `a..b` range (either side may be omitted), a
/// comma-separated choice list, or `UUID`.
fn expand_random(spec: &str, ctx: &PlaceholderCtx) -> Option<String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    if spec.eq_ignore_ascii_case("uuid") {
        return Some((ctx.uuid)());
    }
    let r = unit_sample((ctx.rng)());
    if let Some((lo_s, hi_s)) = spec.split_once("..") {
        let lo: i64 = if lo_s.trim().is_empty() { i64::MIN } else { lo_s.trim().parse().ok()? };
        let hi: i64 = if hi_s.trim().is_empty() { i64::MAX } else { hi_s.trim().parse().ok()? };
        if lo > hi {
            return None;
        }
        let span = hi as i128 - lo as i128 + 1;
        let pick = lo as i128 + (r * span as f64) as i128;
        return Some(pick.min(hi as i128).to_string());
    }
    let choices: Vec<&str> = spec.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if choices.is_empty() {
        return None;
    }
    let idx = ((r * choices.len() as f64) as usize).min(choices.len() - 1);
    Some(choices[idx].to_string())
}

/// Clamp an injected sample into `[0, 1)`, treating NaN and infinities as 0.
fn unit_sample(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, 0.999_999_999_999_999_9)
    } else {
        0.0
    }
}

fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Uniform sample in `[0, 1)` from `/dev/urandom`, falling back to the
/// sub-second clock only if that cannot be read.
fn rand_unit() -> f64 {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let mut b = [0u8; 8];
        if f.read_exact(&mut b).is_ok() {
            let bits = u64::from_le_bytes(b) >> 11; // 53 bits
            return bits as f64 / (1u64 << 53) as f64;
        }
    }
    unit_sample(crate::db::now_ts().fract().abs())
}

/// Local UTC offset in seconds at `t`, via the C library's `localtime_r`
/// (linked transitively, as in src/install.rs; no crate dependency). The
/// `tm_gmtoff` field is part of Darwin's `struct tm`; an unavailable result
/// degrades to UTC so date formatting always produces something.
fn local_offset_secs(t: i64) -> i64 {
    #[repr(C)]
    struct CTm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const std::os::raw::c_char,
    }
    extern "C" {
        fn localtime_r(timep: *const i64, result: *mut CTm) -> *mut CTm;
    }
    let mut tm: CTm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !localtime_r(&t, &mut tm).is_null() };
    if ok {
        tm.tm_gmtoff
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_clipboard(items: &[&str]) -> PlaceholderCtx {
        let mut c = PlaceholderCtx::test();
        c.clipboard = items.iter().map(|s| s.to_string()).collect();
        c
    }

    fn expand(text: &str, ctx: &PlaceholderCtx) -> String {
        expand_placeholders(text, ctx).0
    }

    /// Context whose snippet lookup is a fixed table.
    fn ctx_with_snippets(rows: &[(&str, &str)]) -> PlaceholderCtx {
        let rows: Vec<(String, String)> = rows.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let mut c = PlaceholderCtx::test();
        c.snippet = Some(Box::new(move |kw: &str| {
            rows.iter().find(|(k, _)| k == kw).map(|(_, v)| v.clone())
        }));
        c
    }

    #[test]
    fn placeholder_clipboard_forms() {
        let ctx = ctx_with_clipboard(&["  hello world  ", "MiXeD case", "third"]);
        assert_eq!(expand("{clipboard}", &ctx), "  hello world  ");
        assert_eq!(expand("{clipboard:0}", &ctx), "  hello world  ");
        assert_eq!(expand("{clipboard:1}", &ctx), "MiXeD case");
        assert_eq!(expand("{clipboard:2}", &ctx), "third");
        assert_eq!(expand("{clipboard:trim}", &ctx), "hello world");
        assert_eq!(expand("{clipboard:uppercase}", &ctx), "  HELLO WORLD  ");
        assert_eq!(expand("{clipboard:lowercase}", &ctx), "  hello world  ");
        assert_eq!(expand("{clipboard:capitals}", &ctx), "  Hello World  ");
        assert_eq!(expand("{clipboard:1.trim}", &ctx), "MiXeD case");
        assert_eq!(expand("{clipboard:1.uppercase}", &ctx), "MIXED CASE");
        assert_eq!(expand("{clipboard:1.lowercase}", &ctx), "mixed case");
        // the binary's regex also allows ':' between index and modifier
        assert_eq!(expand("{clipboard:1:uppercase}", &ctx), "MIXED CASE");
        assert_eq!(expand("{clipboard:2.capitals}", &ctx), "Third");
        // case-insensitive keyword and modifier
        assert_eq!(expand("{CLIPBOARD:1}", &ctx), "MiXeD case");
        assert_eq!(expand("{Clipboard:TRIM}", &ctx), "hello world");
        // embedded in larger text
        assert_eq!(expand("say {clipboard:uppercase}!", &ctx), "say   HELLO WORLD  !");
    }

    #[test]
    fn placeholder_clipboard_missing_entries_and_unknown_forms() {
        let empty = PlaceholderCtx::test();
        assert_eq!(expand("{clipboard}", &empty), "");
        assert_eq!(expand("{clipboard:3}", &empty), "");
        let one = ctx_with_clipboard(&["only"]);
        assert_eq!(expand("{clipboard:9}", &one), "");
        // malformed / unknown suffixes stay verbatim
        assert_eq!(expand("{clipboard:bogus}", &one), "{clipboard:bogus}");
        assert_eq!(expand("{clipboard:1.bogus}", &one), "{clipboard:1.bogus}");
        assert_eq!(expand("{clipboard:-1}", &one), "{clipboard:-1}");
        assert_eq!(expand("{clipboard:}", &one), "{clipboard:}");
        assert_eq!(expand("{clipboard:1.}", &one), "{clipboard:1.}");
        assert_eq!(expand("{clipboard:99999999999999999999}", &one), "{clipboard:99999999999999999999}");
        assert_eq!(expand("{clipper}", &one), "{clipper}");
    }

    #[test]
    fn placeholder_date_defaults_and_offsets() {
        let ctx = PlaceholderCtx::test(); // 1970-01-01T00:00:00Z
        assert_eq!(expand("{date}", &ctx), "1970-01-01");
        assert_eq!(expand("{date +1d}", &ctx), "1970-01-02");
        assert_eq!(expand("{date -1d}", &ctx), "1969-12-31");
        assert_eq!(expand("{date -10y:y}", &ctx), "1960");
        assert_eq!(expand("{date +2y +3M +1d:EEEE}", &ctx), "Sunday");
        assert_eq!(expand("{datetime +1h}", &ctx), "1970-01-01 01:00:00");
        assert_eq!(expand("{datetime -1M}", &ctx), "1969-12-01 00:00:00");
        assert_eq!(expand("{datetime +1337h}", &ctx), "1970-02-25 17:00:00");
        assert_eq!(expand("{time}", &ctx), "00:00:00");
        assert_eq!(expand("{time:full}", &ctx), "00:00:00");
        assert_eq!(expand("{isodatetime}", &ctx), "1970-01-01T00:00:00");
        // time-of-day is rendered from the injected clock
        let mut noon = PlaceholderCtx::test();
        noon.now_unix = 45_296; // 1970-01-01T12:34:56Z
        assert_eq!(expand("{date}", &noon), "1970-01-01");
        assert_eq!(expand("{time}", &noon), "12:34:56");
        assert_eq!(expand("{datetime}", &noon), "1970-01-01 12:34:56");
        assert_eq!(expand("{isodatetime}", &noon), "1970-01-01T12:34:56");
        assert_eq!(expand("{date:yyyy-MM-dd}", &noon), "1970-01-01");
    }

    #[test]
    fn placeholder_named_styles() {
        let ctx = PlaceholderCtx::test();
        assert_eq!(expand("{date:short}", &ctx), "1970-01-01");
        assert_eq!(expand("{date:medium}", &ctx), "1 Jan 1970");
        assert_eq!(expand("{date:long}", &ctx), "1 January 1970");
        assert_eq!(expand("{date:full}", &ctx), "Thursday, 1 January 1970");
        assert_eq!(expand("{date +1d:full}", &ctx), "Friday, 2 January 1970");
        assert_eq!(expand("{time:short}", &ctx), "00:00");
        assert_eq!(expand("{time:medium}", &ctx), "00:00:00");
        assert_eq!(expand("{datetime:short}", &ctx), "1970-01-01 00:00");
        assert_eq!(expand("{datetime:full}", &ctx), "Thursday, 1 January 1970 00:00:00");
        // styles are case-insensitive
        assert_eq!(expand("{date:FULL}", &ctx), "Thursday, 1 January 1970");
    }

    #[test]
    fn placeholder_date_pattern_subset() {
        let mut ctx = PlaceholderCtx::test();
        ctx.now_unix = 45_296; // 1970-01-01T12:34:56Z (a Thursday)
        assert_eq!(expand("{date:EEEE}", &ctx), "Thursday");
        assert_eq!(expand("{date:EEE}", &ctx), "Thu");
        assert_eq!(expand("{date:yyyy}", &ctx), "1970");
        assert_eq!(expand("{date:yy}", &ctx), "70");
        assert_eq!(expand("{date:y}", &ctx), "1970");
        assert_eq!(expand("{date:MMMM}", &ctx), "January");
        assert_eq!(expand("{date:MMM}", &ctx), "Jan");
        assert_eq!(expand("{date:MM}", &ctx), "01");
        assert_eq!(expand("{date:M}", &ctx), "1");
        assert_eq!(expand("{date:dd}", &ctx), "01");
        assert_eq!(expand("{date:d}", &ctx), "1");
        assert_eq!(expand("{time:HH:mm:ss}", &ctx), "12:34:56");
        assert_eq!(expand("{time:H:m:s}", &ctx), "12:34:56");
        assert_eq!(expand("{time:HH:mm}", &ctx), "12:34");
        // unrecognised pattern letters (and unsupported widths) are literal;
        // `y`, `M`, `d`, `H`, `m`, `s` and `E` are real pattern letters
        assert_eq!(expand("{date:xJz}", &ctx), "xJz");
        assert_eq!(expand("{date:xyz}", &ctx), "x1970z");
        assert_eq!(expand("{date:yyyy!}", &ctx), "1970!");
        assert_eq!(expand("{date:EE}", &ctx), "EE");
        assert_eq!(expand("{datetime:yyyy-MM-dd'T'HH:mm}", &ctx), "1970-01-01'T'12:34");
    }

    #[test]
    fn placeholder_date_offsets_are_bounded_and_invalid_offsets_verbatim() {
        let ctx = PlaceholderCtx::test();
        assert_eq!(expand("{date +1q}", &ctx), "{date +1q}");
        assert_eq!(expand("{date 1d}", &ctx), "{date 1d}");
        assert_eq!(expand("{date +d}", &ctx), "{date +d}");
        assert_eq!(expand("{date +1.5d}", &ctx), "{date +1.5d}");
        // offset with a non-ASCII tail must not panic and stays verbatim
        assert_eq!(expand("{date +1é}", &ctx), "{date +1é}");
        assert_eq!(expand("{date +9223372036854775807y}", &ctx), "{date +9223372036854775807y}");
    }

    #[test]
    fn placeholder_random_ranges_and_choices() {
        let fixed = |v: f64| {
            let mut c = PlaceholderCtx::test();
            c.rng = Box::new(move || v);
            c
        };
        assert_eq!(expand("{random:1..10}", &fixed(0.0)), "1");
        assert_eq!(expand("{random:1..10}", &fixed(0.999_999_999_999_999_9)), "10"); // inclusive
        assert_eq!(expand("{random:0..}", &fixed(0.0)), "0");
        assert_eq!(expand("{random:..-1}", &fixed(0.0)), i64::MIN.to_string());
        assert_eq!(expand("{random:..}", &fixed(0.0)), i64::MIN.to_string());
        // an open upper bound samples near the top of the i64 range; an f64
        // fraction cannot resolve the last few units of a 2^64 span, so assert
        // the reachable neighbourhood rather than exact equality
        let top: i64 = expand("{random:..}", &fixed(0.999_999_999_999_999_9)).parse().unwrap();
        assert!(top > i64::MAX / 2, "got {top}");
        assert_eq!(expand("{random:apple,banana,pear}", &fixed(0.0)), "apple");
        assert_eq!(expand("{random:apple,banana,pear}", &fixed(0.99)), "pear");
        assert_eq!(expand("{random: apple , banana , pear }", &fixed(0.99)), "pear");
        assert_eq!(expand("{random:only}", &fixed(0.5)), "only");
        // malformed specs stay verbatim
        assert_eq!(expand("{random:}", &fixed(0.0)), "{random:}");
        assert_eq!(expand("{random:5..1}", &fixed(0.0)), "{random:5..1}");
        assert_eq!(expand("{random:a..b}", &fixed(0.0)), "{random:a..b}");
        assert_eq!(expand("{random:1..2..3}", &fixed(0.0)), "{random:1..2..3}");
        assert_eq!(expand("{random:,}", &fixed(0.0)), "{random:,}");
        // a non-finite injected sample must not panic or escape the range
        assert_eq!(expand("{random:1..10}", &fixed(f64::NAN)), "1");
    }

    #[test]
    fn placeholder_random_uuid() {
        let ctx = PlaceholderCtx::test();
        assert_eq!(expand("{random:UUID}", &ctx), "00000000-0000-4000-8000-000000000000");
        assert_eq!(expand("{random:uuid}", &ctx), "00000000-0000-4000-8000-000000000000");
        // the production generator is wired in and yields a version-4 UUID
        let mut live = PlaceholderCtx::test();
        live.uuid = Box::new(crate::db::uuid_v4);
        let s = expand("{random:UUID}", &live);
        assert_eq!(s.len(), 36, "got {s}");
        assert_eq!(s.as_bytes()[14], b'4', "got {s}");
        assert!(matches!(s.as_bytes()[19], b'8' | b'9' | b'a' | b'b'), "got {s}");
        // a one-item choice list is still a choice list
        assert_eq!(expand("{random:random}", &ctx), "random");
    }

    #[test]
    fn placeholder_cursor_offset() {
        let ctx = PlaceholderCtx::test();
        assert_eq!(expand_placeholders("abc{cursor}def", &ctx), ("abcdef".to_string(), Some(3)));
        assert_eq!(expand_placeholders("def{cursor}", &ctx), ("def".to_string(), Some(3)));
        assert_eq!(expand_placeholders("{cursor}abc", &ctx), ("abc".to_string(), Some(0)));
        assert_eq!(expand_placeholders("ab {cursor} cd", &ctx), ("ab  cd".to_string(), Some(3)));
        assert_eq!(expand_placeholders("no marker here", &ctx), ("no marker here".to_string(), None));
        // the offset is a character index, not a byte index
        assert_eq!(expand_placeholders("é{cursor}x", &ctx), ("éx".to_string(), Some(1)));
        // the first marker wins; later markers are still removed
        assert_eq!(expand_placeholders("a{cursor}b{cursor}c", &ctx), ("abc".to_string(), Some(1)));
        // a marker after another placeholder accounts for the expansion
        let clip = ctx_with_clipboard(&["xy"]);
        assert_eq!(expand_placeholders("{clipboard}{cursor}!", &clip), ("xy!".to_string(), Some(2)));
    }

    #[test]
    fn placeholder_unknown_tokens_stay_verbatim() {
        let ctx = PlaceholderCtx::test();
        for t in ["{nope}", "{}", "{ }", "{clipboard extra}", "{datex}", "{random}", "{snippet:}", "{", "}", "a{b", "}{"] {
            assert_eq!(expand(t, &ctx), t, "input {t}");
        }
    }

    #[test]
    fn placeholder_snippet_nesting_and_recursion_bound() {
        // a snippet whose body contains its own keyword must stop (SPEC §8.11)
        let recursive = ctx_with_snippets(&[("a", "x {snippet:a} y")]);
        let got = expand("{snippet:a}", &recursive);
        assert_eq!(got, "x x x x {snippet:a} y y y y", "unexpected expansion: {got}");
        assert_eq!(got.matches("{snippet:a}").count(), 1);
        assert!(MAX_PLACEHOLDER_DEPTH >= 2 && MAX_PLACEHOLDER_DEPTH <= 16);
        // mutual recursion also terminates
        let mutual = ctx_with_snippets(&[("p", "P{snippet:q}"), ("q", "Q{snippet:p}")]);
        let got = expand("{snippet:p}", &mutual);
        assert!(got.matches("P").count() <= MAX_PLACEHOLDER_DEPTH);
        assert!(got.matches("Q").count() <= MAX_PLACEHOLDER_DEPTH);
        // ordinary nesting expands fully
        let chain = ctx_with_snippets(&[("s1", "A{snippet:s2}"), ("s2", "B")]);
        assert_eq!(expand("pre {snippet:s1} post", &chain), "pre AB post");
        // placeholders inside a snippet body are expanded too
        let inner = ctx_with_snippets(&[("d", "on {date}")]);
        assert_eq!(expand("{snippet:d}", &inner), "on 1970-01-01");
        // an unknown keyword stays verbatim
        assert_eq!(expand("{snippet:nope}", &inner), "{snippet:nope}");
        // no lookup configured at all: still verbatim, never a panic
        assert_eq!(expand("{snippet:d}", &PlaceholderCtx::test()), "{snippet:d}");
    }

    #[test]
    fn placeholder_injects_time_and_offset() {
        let mut ctx = PlaceholderCtx::test();
        ctx.now_unix = 1_000_000_000; // 2001-09-09T01:46:40Z
        assert_eq!(expand("{date}", &ctx), "2001-09-09");
        assert_eq!(expand("{time}", &ctx), "01:46:40");
        assert_eq!(expand("{isodatetime}", &ctx), "2001-09-09T01:46:40");
        // an explicit UTC offset moves the civil day
        let mut plus8 = PlaceholderCtx::test();
        plus8.now_unix = 20 * 3600; // 1970-01-01T20:00:00Z
        plus8.utc_offset_secs = 8 * 3600;
        assert_eq!(expand("{date}", &plus8), "1970-01-02");
        assert_eq!(expand("{time}", &plus8), "04:00:00");
    }

    #[test]
    fn placeholder_mixed_text_and_idempotence() {
        let mut ctx = ctx_with_clipboard(&["there"]);
        ctx.now_unix = 0;
        assert_eq!(
            expand("Hi {clipboard:uppercase}, today is {date}.", &ctx),
            "Hi THERE, today is 1970-01-01."
        );
        // unknown tokens are stable under a second pass
        let once = expand("keep {nope} as is", &ctx);
        assert_eq!(expand(&once, &ctx), once);
        // plain text is unchanged
        assert_eq!(expand("no placeholders", &ctx), "no placeholders");
        assert_eq!(expand("", &ctx), "");
    }

    #[test]
    fn placeholder_never_panics_on_malformed_or_large_input() {
        let ctx = ctx_with_clipboard(&["x"]);
        let nasty = [
            "{clipboard", "{clipboard:", "{clipboard:1.", "{clipboard:.}", "{date +}", "{date:}", "{date::}",
            "{random:..}", "{random:..-}", "{snippet", "{cursor", "{{\u{1F600}}}", "{clipboard:é}", "{date +é}",
            "\u{0}{clipboard}\u{0}", "🦀{date}🦀", "{time:HH:mm:ss.SSS}", "{isodatetime x}",
        ];
        for t in nasty {
            let (out, _) = expand_placeholders(t, &ctx);
            assert!(out.chars().count() <= t.chars().count() * 4 + 64, "grew oddly: {t} -> {out}");
        }
        let big = format!("{} {{date}} {}", "x".repeat(100_000), "y".repeat(100_000));
        let (out, _) = expand_placeholders(&big, &ctx);
        assert!(out.contains("1970-01-01"));
        assert_eq!(out.chars().count(), big.chars().count() - "{date}".len() + 10);
    }

    #[test]
    fn placeholder_default_context_is_sane() {
        // the local offset is a real one, and the default path renders without
        // touching any store
        assert!(local_offset_secs(0).abs() <= 14 * 3600);
        assert_eq!(expand_placeholders_default("plain text"), "plain text");
        let (out, cur) = expand_placeholders("{date} {time}", &PlaceholderCtx::default());
        assert!(out.len() >= 11, "got {out}");
        assert_eq!(cur, None);
        // the civil conversions round-trip
        for days in [-1_000_000i64, -1, 0, 1, 719_468, 1_000_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(weekday_from_days(0), 4); // Thursday
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn placeholder_utf8_boundaries_are_preserved() {
        let ctx = ctx_with_clipboard(&["naïve 🦀"]);
        assert_eq!(expand("🦀{clipboard}🦀", &ctx), "🦀naïve 🦀🦀");
        assert_eq!(expand("{clipboard:uppercase}", &ctx), "NAÏVE 🦀");
        assert_eq!(expand("{clipboard:capitals}", &ctx), "Naïve 🦀");
        assert_eq!(expand("{clipboard:1}", &ctx), "");
        assert_eq!(expand_placeholders("é{cursor}é", &ctx).0, "éé");
    }

    #[test]
    fn match_expand_full_reports_the_ignore_flag() {
        let rows = vec![
            ("omw".to_string(), "On my way!".to_string(), true),
            ("sig".to_string(), "Regards".to_string(), false),
        ];
        assert_eq!(
            match_expand_rows_full("omw", &rows),
            Some(("omw".to_string(), "On my way!".to_string(), true))
        );
        assert_eq!(
            match_expand_rows_full("sig", &rows),
            Some(("sig".to_string(), "Regards".to_string(), false))
        );
        assert_eq!(match_expand_rows_full("nope", &rows), None);
        // The two-element form still serves callers that do not need the flag.
        let two = vec![("omw".to_string(), "x".to_string())];
        assert_eq!(
            match_expand_rows("omw", &two),
            Some(("omw".to_string(), "x".to_string()))
        );
    }

    #[test]
    fn keyword_match_does_not_slice_multibyte_boundaries() {
        // A buffer ending in a multi-byte character used to be sliced at a
        // non-char boundary while checking for a keyword suffix (panic).
        let rows = vec![("ab".to_string(), "x".to_string())];
        assert!(match_expand_rows("éab", &rows).is_some());
        assert_eq!(match_expand_rows("🦀", &rows), None);
        assert_eq!(match_expand_rows("é", &rows), None);
        let rows3 = vec![("ab".to_string(), "x".to_string(), false)];
        assert!(match_expand_rows_full("🦀ab", &rows3).is_some());
    }
}
