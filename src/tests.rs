use crate::calc::{calculate, is_calc};
use crate::search::fuzzy_score;

#[test]
fn arithmetic() {
    assert_eq!(calculate("1+2*3").unwrap(), "7");
    assert_eq!(calculate("(2+3)*6").unwrap(), "30");
    assert_eq!(calculate("2^10").unwrap(), "1024");
    assert_eq!(calculate("sqrt(144) + 10").unwrap(), "22");
    assert_eq!(calculate("10 % 3").unwrap(), "1");
    assert_eq!(calculate("-5 + 2").unwrap(), "-3");
}

#[test]
fn conversions() {
    let r = calculate("1 gb to mb").unwrap();
    assert!(r.starts_with("1 gb = 1024 mb"), "got {r}");
    let r = calculate("3.5km in m").unwrap();
    assert!(r.contains("3500"), "got {r}");
    let r = calculate("1 hour to s").unwrap();
    assert!(r.contains("3600"), "got {r}");
    assert!(calculate("1 m to s").is_none());
}

#[test]
fn detector() {
    assert!(is_calc("1+2"));
    assert!(is_calc("(3)*4"));
    assert!(!is_calc("safari"));
    assert!(!is_calc("g query"));
}

#[test]
fn invalid() {
    assert!(calculate("1/0").is_none());
    assert!(calculate("2 * * 3").is_none());
    assert!(calculate("1 + abc").is_none());
    assert!(calculate("(1").is_none());
    assert_eq!(calculate("2 + + 3").unwrap(), "5"); // unary plus
}

#[test]
fn fuzz() {
    assert!(fuzzy_score("saf", "Safari") > 100.0);
    assert!(fuzzy_score("saf", "Firefox") == 0.0);
    assert!(fuzzy_score("tc", "Text Console") > 0.0);
    assert!(fuzzy_score("abc", "abc") > fuzzy_score("abc", "abcdef"));
}

#[test]
fn expand_match() {
    let rows = vec![
        ("omw".to_string(), "On my way!".to_string()),
        ("thx".to_string(), "Thanks!".to_string()),
    ];
    // plain suffix match at start
    assert_eq!(crate::expand::match_expand_rows("omw", &rows),
        Some(("omw".into(), "On my way!".into())));
    // case-insensitive
    assert_eq!(crate::expand::match_expand_rows("OMW", &rows).map(|(k, _)| k),
        Some("omw".into()));
    // word boundary: preceded by space is fine
    assert!(crate::expand::match_expand_rows("talk omw", &rows).is_some());
    // word boundary violated: embedded inside a longer word
    assert!(crate::expand::match_expand_rows("romwx", &rows).is_none());
    assert!(crate::expand::match_expand_rows("xomw", &rows).is_none());
    // min length 2, no partial
    assert!(crate::expand::match_expand_rows("th", &rows).is_none());
    assert!(crate::expand::match_expand_rows("", &rows).is_none());
    // buffer tail after more text: keyword must be exactly at tail
    assert!(crate::expand::match_expand_rows("omw right now", &rows).is_none());
}

#[test]
fn expand_keycodes() {
    // verified against Carbon Events.h kVK_ANSI_*
    assert_eq!(crate::expand::us_char(0, false), Some('a'));
    assert_eq!(crate::expand::us_char(12, false), Some('q'));
    assert_eq!(crate::expand::us_char(18, false), Some('1'));
    assert_eq!(crate::expand::us_char(18, true), Some('!'));
    assert_eq!(crate::expand::us_char(31, false), Some('o'));
    assert_eq!(crate::expand::us_char(0x2F, false), Some('.'));
    assert_eq!(crate::expand::us_char(99, false), None);
}

#[test]
fn expand_watcher_buffer() {
    let mut w = crate::expand::Watcher::new();
    w.simulate = true;
    w.rows = vec![("omw".into(), "On my way!".into())];
    // type o m w: kVK_ANSI_O=31, M=46, W=13 (Carbon Events.h)
    w.on_key(31, false);
    w.on_key(46, false);
    w.on_key(13, false);
    // hitting Space (49) fires expansion: erase kw typed + paste. In test env
    // the synthetic events post into an inert source; the decision itself is
    // what we assert — returns Some(omw).
    let fired = w.on_key(49, false);
    assert_eq!(fired.as_deref(), Some("omw"));
    assert!(w.buffer.is_empty());
    // no match: random typing
    w.on_key(1, false);
    assert_eq!(w.on_key(49, false), None);
}

#[test]
fn contacts_fixture_and_parse() {
    // parse three-list AppleScript output
    let p = crate::contacts::parse_applescript_list(
        "{Ada Lovelace, Ada Smith}, {+15550101, }, {ada@example.com, asmith@example.com}", 8);
    assert_eq!(p[0].0, "Ada Lovelace");
    assert_eq!(p[0].1, "+15550101");
    assert_eq!(p[1].1, "");
    assert_eq!(p[1].2, "asmith@example.com");
    // single-list fallback
    let q = crate::contacts::parse_applescript_list("{Alice, Bob}", 8);
    assert_eq!(q.len(), 2);
    assert_eq!(q[0].0, "Alice");
    // fixture-backed candidate pipeline
    let f = std::env::temp_dir().join("rf_contacts_test.json");
    std::fs::write(&f, r#"[{"name":"Ada Lovelace","phone":"+15550101","email":""},{"name":"Ada Smith","phone":"","email":"a@b.c"}]"#).unwrap();
    std::env::set_var("RALFRED_CONTACTS_JSON", &f);
    let cands = crate::contacts::candidates("c ada");
    assert_eq!(cands.len(), 2);
    // phone-first action, email fallback
    assert!(cands[0].action.as_deref().unwrap().contains("tel:+15550101"));
    assert!(cands[1].action.as_deref().unwrap().contains("mailto:a@b.c"));
    // keyword discipline
    assert!(crate::contacts::candidates("contactz ada").is_empty());
    assert!(crate::contacts::candidates("c").is_empty());
    std::env::remove_var("RALFRED_CONTACTS_JSON");
    std::fs::remove_file(&f).ok();
}

#[test]
fn itunes_verbs_and_fixture() {
    // pure verb mapping (evidence strings: play/pause/playpause/next...)
    assert_eq!(crate::itunes::verb_command("play"), Some("play"));
    assert_eq!(crate::itunes::verb_command("PLAYPAUSE"), Some("playpause"));
    assert_eq!(crate::itunes::verb_command("prev"), Some("previous track"));
    assert_eq!(crate::itunes::verb_command("dance"), None);
    // fixture library search
    let f = std::env::temp_dir().join("rf_music_test.json");
    std::fs::write(&f, r#"[{"name":"Bohemian Rhapsody","artist":"Queen","album":"ANATO"},{"name":"Imagine","artist":"John Lennon","album":"Imagine"}]"#).unwrap();
    std::env::set_var("RALFRED_MUSIC_JSON", &f);
    let c = crate::itunes::candidates("music queen");
    assert_eq!(c.len(), 1);
    assert!(c[0].title.contains("Bohemian"));
    assert!(c[0].action.as_deref().unwrap().starts_with("music:playbyname:"));
    // verb candidate carries action tag
    let v = crate::itunes::candidates("music pause");
    assert_eq!(v[0].action.as_deref(), Some("music:pause"));
    // keyword discipline: not for other keywords
    assert!(crate::itunes::candidates("musicx queen").is_empty());
    std::env::remove_var("RALFRED_MUSIC_JSON");
    std::fs::remove_file(&f).ok();
}

#[test]
fn quicklook_url_and_toggle() {
    // real file -> canonical file://
    let u = crate::quicklook::preview_url(Some("/etc/hosts"), None).unwrap();
    assert!(u.starts_with("file:///"));
    assert!(u.ends_with("hosts"));
    // missing path + text -> materialized temp file
    let u2 = crate::quicklook::preview_url(Some("/nonexistent/zz"), Some("hello ql"))
        .unwrap();
    assert!(u2.starts_with("file://"));
    let p = u2.strip_prefix("file://").unwrap();
    assert_eq!(std::fs::read_to_string(p).unwrap(), "hello ql");
    // no path no text -> None
    assert!(crate::quicklook::preview_url(None, None).is_none());
    // toggle state machine (no-exec mode): open, same->close, reopen, switch
    std::env::set_var("RALFRED_QL_NOEXEC", "1");
    let open = crate::quicklook::toggle("file:///tmp/a").unwrap();
    assert_eq!(open, "file:///tmp/a");
    assert!(crate::quicklook::toggle("file:///tmp/a").is_none()); // second press closes
    let again = crate::quicklook::toggle("file:///tmp/a").unwrap();
    assert_eq!(again, "file:///tmp/a");
    let other = crate::quicklook::toggle("file:///tmp/b").unwrap(); // switch
    assert_eq!(other, "file:///tmp/b");
    assert!(crate::quicklook::toggle("file:///tmp/b").is_none());
    std::env::remove_var("RALFRED_QL_NOEXEC");
}

#[test]
fn icons_and_onepassword() {
    // icons: icns discovery (dir bundle -> icns), rasterize + cache hit
    let app = std::path::PathBuf::from("/System/Applications/Utilities/Terminal.app");
    if app.exists() {
        let ic = crate::icons::icns_for(&app);
        assert!(ic.is_some());
        let png = crate::icons::icon_png(&app);
        assert!(png.is_some());
        assert_eq!(png, crate::icons::icon_png(&app)); // cache: same path back
        let h = png.unwrap();
        let head = std::fs::read(&h).unwrap();
        assert_eq!(&head[0..4], b"\x89PNG");
    }
    // non-existent path: no icns, None icon
    assert!(crate::icons::icns_for(std::path::Path::new("/zz/nope")).is_none());
    // onepassword: fixture items get fill actions; otherwise search scheme
    let f = std::env::temp_dir().join("rf_op_test.json");
    std::fs::write(&f, r#"[{"id":"G1","vault":"P","title":"GitHub","user":"u@x.y"},{"id":"A2","vault":"W","title":"AWS","user":"ops"}]"#).unwrap();
    std::env::set_var("RALFRED_OP_JSON", &f);
    let c = crate::onepassword::candidates("op github");
    assert_eq!(c.len(), 1);
    assert!(c[0].action.as_deref().unwrap().starts_with("onepassword:fill/P/G1"));
    let s = crate::onepassword::candidates("op slack");
    assert!(s[0].action.as_deref().unwrap().starts_with("onepassword:search:slack"));
    assert!(crate::onepassword::candidates("ops github").is_empty());
    assert!(crate::onepassword::candidates("op").len() == 1); // bare -> hint card
    std::env::remove_var("RALFRED_OP_JSON");
    std::fs::remove_file(&f).ok();
}

pub(crate) static INSTALL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn install_lifecycle_sandboxed() {
    // serialized: install state is process-global (sandbox root / env)
    let _lock = INSTALL_TEST_LOCK.lock().unwrap();
    let home = std::env::temp_dir().join(format!("rf_install_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    // pin via API, not env, so it can't race other tests reading RALFRED_HOME
    crate::install::use_sandbox(&home);
    std::env::set_var("RALFRED_INSTALL_NOLAUNCHD", "1");
    assert_eq!(crate::install::status(), "not installed");
    crate::install::install("ALT+CTRL+SPACE").unwrap();
    let st = crate::install::status();
    assert!(st.starts_with("installed (plist)"), "got {st}");
    let plist = home.join("LaunchAgents").join("com.kleo.ralfred.plist");
    let xml = std::fs::read_to_string(&plist).unwrap();
    assert!(xml.contains("com.kleo.ralfred"));
    assert!(xml.contains("gui"));
    assert!(xml.contains("ALT+CTRL+SPACE"));
    assert!(xml.contains("<key>RunAtLoad</key>"));
    // plist is valid enough for plutil to parse it
    let ok = std::process::Command::new("/usr/bin/plutil")
        .args(["-lint", &plist.to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status().map(|s| s.success()).unwrap_or(false);
    assert!(ok, "plist lint");
    // idempotent uninstall + double-install overwrite
    crate::install::install("CMD+ALT+K").unwrap();
    assert!(std::fs::read_to_string(&plist).unwrap().contains("CMD+ALT+K"));
    crate::install::uninstall().unwrap();
    assert_eq!(crate::install::status(), "not installed");
    crate::install::uninstall().unwrap(); // idempotent
    std::env::remove_var("RALFRED_INSTALL_NOLAUNCHD");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn webbookmarks_fixture() {
    let f = std::env::temp_dir().join("rf_bm_test.json");
    std::fs::write(&f, r#"[{"title":"Rust Book","url":"https://doc.rust-lang.org/book/"},{"title":"arXiv","url":"https://arxiv.org"}]"#).unwrap();
    std::env::set_var("RALFRED_BM_JSON", &f);
    let c = crate::webbookmarks::candidates("bm rust");
    assert_eq!(c.len(), 1);
    assert!(c[0].action.as_deref().unwrap().starts_with("openurl:https://doc.rust-lang"));
    // url substring search
    let u = crate::webbookmarks::candidates("bm arxiv.org");
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].title, "arXiv");
    // bare + keywords
    assert!(crate::webbookmarks::candidates("bm").len() == 1);
    assert!(crate::webbookmarks::candidates("bookmarks rust").len() == 1);
    assert!(crate::webbookmarks::candidates("bmzz rust").is_empty());
    // no-match fixture -> empty (never falls through to live in tests)
    assert!(crate::webbookmarks::candidates("bm zzznope").is_empty());
    std::env::remove_var("RALFRED_BM_JSON");
    std::fs::remove_file(&f).ok();
}

#[test]
fn largetype_candidates_and_scale() {
    // font scale auto-fit (Alfred behavior, our thresholds)
    assert_eq!(crate::largetype::font_size("42"), 180);
    assert_eq!(crate::largetype::font_size("ten charxs"), 110); // 10 chars
    assert_eq!(crate::largetype::font_size("eleven chars!!"), 64); // 14
    assert!(crate::largetype::font_size("x".repeat(25).as_str()) <= 40);
    // routing candidate
    let c = crate::largetype::candidates("lt hello");
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].action.as_deref(), Some("largetype:hello"));
    assert!(crate::largetype::candidates("lt").len() == 1);
    assert!(crate::largetype::candidates("largetype y").len() == 1);
    assert!(crate::largetype::candidates("ltx y").is_empty());
    assert!(crate::largetype::candidates("lt").len() == 1);
    // "lt " (trailing space) -> still the hint card, never an empty-text overlay
    let h = crate::largetype::candidates("lt ");
    assert_eq!(h.len(), 1);
    assert!(h[0].action.is_none());
}

#[test]
fn hotkey_combo_robust_parsing() {
    // normal
    assert!(crate::hotkey::parse_combo("ALT+CTRL+SPACE").is_ok());
    // trailing junk from the user's mis-closed quote
    assert!(crate::hotkey::parse_combo("ALT+CTRL+SPACE/").is_ok());
    assert!(crate::hotkey::parse_combo("CTRL+/").is_ok());
    assert!(crate::hotkey::parse_combo("alt+ctrl+k").is_ok());
    // still rejects garbage
    assert!(crate::hotkey::parse_combo("JUSTKEY").is_err());
    assert!(crate::hotkey::parse_combo("ALT+BADKEY123X").is_err());
}

#[test]
fn settings_get_patch_roundtrip() {
    let _lock = INSTALL_TEST_LOCK.lock().unwrap();
    let home = std::env::temp_dir().join(format!("rf_set_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    crate::install::use_sandbox(&home);
    std::env::set_var("RALFRED_HOME", &home);
    std::env::set_var("RALFRED_INSTALL_NOLAUNCHD", "1");
    // get exposes the documented fields + schema sections
    let v = crate::settings::get();
    assert_eq!(v["hotkey"], "ALT+CTRL+SPACE");
    let s = crate::settings::schema();
    assert!(s["sections"].as_array().unwrap().len() >= 4);
    // patch: valid + invalid
    assert!(crate::settings::patch(&serde_json::json!({"hotkey": "CMD+ALT+K"})).is_ok());
    assert_eq!(crate::settings::get()["hotkey"], "CMD+ALT+K");
    assert!(crate::settings::patch(&serde_json::json!({"hotkey": "BAD+"})).is_err());
    assert!(crate::settings::patch(&serde_json::json!({"theme": "neon"})).is_err());
    assert!(crate::settings::patch(&serde_json::json!({"max_results": 1000})).is_err());
    assert!(crate::settings::patch(&serde_json::json!({"auto_expand_snippets": false})).is_ok());
    assert_eq!(crate::settings::get()["auto_expand_snippets"], false);
    // persistence: Config::load sees the change
    assert_eq!(crate::config::Config::load().hotkey, "CMD+ALT+K");
    std::env::remove_var("RALFRED_HOME");
    std::env::remove_var("RALFRED_INSTALL_NOLAUNCHD");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn features_enable_disable_gating() {
    let _lock = INSTALL_TEST_LOCK.lock().unwrap();
    let home = std::env::temp_dir().join(format!("rf_feat_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    // NB: no install::use_sandbox here — it pins a process-wide OnceCell and
    // would steal the pin from the install/settings/migrate tests. RALFRED_HOME
    // alone isolates features/config/router, which is all this test touches.
    std::env::set_var("RALFRED_HOME", &home);

    // list exposes the full feature-domain inventory, default all enabled
    let doc = crate::features::list();
    let arr = doc.as_array().unwrap();
    assert!(arr.len() >= 13);
    assert!(arr.iter().all(|f| f["enabled"].as_bool().unwrap_or(true)));
    assert!(arr.iter().any(|f| f["id"] == "largetype"));

    // patch disables, persists, and the router honors it
    assert!(crate::features::patch("largetype", false).is_ok());
    assert!(!crate::features::enabled("largetype"));
    assert!(crate::features::enabled("contacts"));
    let cands = crate::router::route("lt hello", &crate::config::Config::load(), &crate::db::Store::knowledge());
    assert!(!cands.iter().any(|c| c.title.starts_with("Large Type")), "disabled feature leaked into results");
    let _ = crate::features::patch("nope-not-real", true);
    assert!(crate::features::patch("nope-not-real", true).is_err());

    // re-enable restores routing
    assert!(crate::features::patch("largetype", true).is_ok());
    std::env::remove_var("RALFRED_HOME");
    let _ = std::fs::remove_dir_all(&home);
}
