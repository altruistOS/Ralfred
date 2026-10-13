//! Native GUI — tao window + wry webview (WKWebView).
//! Model from Alfred: borderless rounded panel, always-on-top, hides on blur,
//! global hotkey summons.
//!
//! All web<->rust traffic goes through the "ralfred" custom protocol (fetch
//! endpoints): evaluate_script callbacks and custom-scheme navigation proved
//! unreliable; the synchronous protocol handler is the documented wry path.

use crate::db::Store;
use crate::engine;
use crate::search::Candidate;
use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::http::{Response, StatusCode};
use wry::WebViewBuilder;
use serde_json::Value;

#[derive(Clone)]
pub struct UserEvent(u32); // SHOW=1 HIDE=2 LT=3

pub const SHOW: u32 = 1;
pub const HIDE: u32 = 2;
pub const LT: u32 = 3;
pub const RESYNC: u32 = 4;

static DOM_ROWS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static ST_PROBE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static EXEC_ACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static THEME_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Default)]
struct Shared {
    items: Mutex<Vec<Candidate>>,
    lt: Mutex<Option<String>>,
}

pub fn run(hotkey: Option<&str>) -> Result<(), String> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let shared = Arc::new(Shared::default());

    let mut builder = WindowBuilder::new()
        .with_title("ralfred")
        .with_transparent(true)
        .with_decorations(false)
        .with_always_on_top(true)
        .with_visible(false)
        .with_inner_size(tao::dpi::LogicalSize::new(680.0, 58.0));
    if let Some(m) = event_loop.primary_monitor() {
        let size = m.size().to_logical::<f64>(m.scale_factor());
        builder = builder.with_position(tao::dpi::LogicalPosition::new(
            (size.width - 680.0) / 2.0,
            size.height * 0.22,
        ));
    }
    let window = builder.build(&event_loop).map_err(|e| format!("window: {e}"))?;

    // ---- custom protocol: html + api on one scheme ----
    let sh = shared.clone();
    let proxy_h: EventLoopProxy<UserEvent> = event_loop.create_proxy();
    let webview = WebViewBuilder::new()
        .with_custom_protocol("ralfred".to_string(), move |_id, req| {
            let uri = req.uri();
            let path = uri.path().to_string();
            let query = uri.query().unwrap_or("").to_string();
            if std::env::var("RALFRED_PROTO_DEBUG").is_ok() {
                eprintln!("[proto] {path}?{query}");
            }
            let body: Cow<'static, [u8]> = match path.as_str() {
                "/search" => {
                    let q = url_decode(query.strip_prefix("q=").unwrap_or(&query));
                    let store = Store::knowledge();
                    let cfg = crate::config::Config::load();
                    let cands = crate::router::route(&q, &cfg, &store);
                    let json = engine::items_json(&cands);
                    *sh.items.lock().unwrap() = cands;
                    return Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .header("access-control-allow-origin", "*")
                        .body(Cow::Owned(json.into_bytes()))
                        .unwrap();
                }
                "/exec" => {
                    let n: usize = query
                        .strip_prefix("id=")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let c = sh.items.lock().unwrap().get(n).cloned();
                    match c {
                        Some(c) => {
                            // large type: full-screen overlay instead of hide/exec
                            if let Some(tag) = &c.action {
                                if let Some(text) = tag.strip_prefix("largetype:") {
                                    *sh.lt.lock().unwrap() = Some(text.to_string());
                                    let _ = proxy_h.send_event(UserEvent(LT));
                                    EXEC_ACK.store(true, std::sync::atomic::Ordering::SeqCst);
                                    return Response::builder().status(StatusCode::OK)
                                        .header("content-type","application/json")
                                        .body(Cow::Owned(br#"{"ok":true,"largetype":true}"#.to_vec())).unwrap();
                                }
                            }
                            let store = Store::knowledge();
                            engine::act(&c, &store);
                            EXEC_ACK.store(true, std::sync::atomic::Ordering::SeqCst);
                            let _ = proxy_h.send_event(UserEvent(HIDE));
                        }
                        None => {
                            return Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(Cow::Owned(br#"{"ok":false}"#.to_vec()))
                                .unwrap()
                        }
                    }
                    Cow::Owned(br#"{"ok":true}"#.to_vec())
                }
                "/ql" => {
                    let n: usize = query
                        .strip_prefix("id=")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let c = sh.items.lock().unwrap().get(n).cloned();
                    if let Some(c) = c {
                        if let Some(u) = crate::quicklook::preview_url(c.path.as_deref(), Some(&c.subtitle)) {
                            crate::quicklook::toggle(&u);
                        }
                    }
                    Cow::Owned(Vec::new())
                }
                "/icon" => {
                    let raw = path.strip_prefix("/icon/").unwrap_or("").to_string();
                    let p = url_decode(&raw);
                    let png = crate::icons::icon_png(std::path::Path::new(&p));
                    match png {
                        Some(f) => {
                            let bytes = std::fs::read(&f).unwrap_or_default();
                            return Response::builder()
                                .status(StatusCode::OK)
                                .header("content-type", "image/png")
                                .header("access-control-allow-origin", "*")
                                .body(Cow::Owned(bytes))
                                .unwrap();
                        }
                        None => return Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Cow::Owned(Vec::new()))
                            .unwrap(),
                    }
                }
                "/settings" => {
                    if req.method() == wry::http::Method::PATCH {
                        let body: Value = serde_json::from_slice(req.body()).unwrap_or(Value::Null);
                        match crate::settings::patch(&body) {
                            Ok(()) => Cow::Owned(br#"{"ok":true}"#.to_vec()),
                            Err(e) => return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .header("content-type", "application/json")
                                .body(Cow::Owned(format!("{{\"ok\":false,\"error\":\"{e}\"}}").into_bytes()))
                                .unwrap(),
                        }
                    } else {
                        let doc = serde_json::json!({"schema": crate::settings::schema(), "values": crate::settings::get()});
                        return Response::builder()
                            .status(StatusCode::OK)
                            .header("content-type", "application/json")
                            .header("access-control-allow-origin", "*")
                            .header("access-control-allow-methods", "GET, PATCH, OPTIONS")
                            .body(Cow::Owned(doc.to_string().into_bytes()))
                            .unwrap();
                    }
                }
                "/hide" => {
                    let _ = proxy_h.send_event(UserEvent(HIDE));
                    Cow::Owned(Vec::new())
                }
                "/probe" => {
                    if let Some(r) = query.strip_prefix("rows=") {
                        DOM_ROWS.store(r.parse().unwrap_or(0), std::sync::atomic::Ordering::SeqCst);
                    }
                    if let Some(b) = query.strip_prefix("bg=") {
                        // end-to-end theme check: served HTML's RF_BG vs resolved theme
                        let want = crate::theme::Theme::resolve(&crate::config::Config::load().theme)
                            .background;
                        THEME_OK.store(b.eq_ignore_ascii_case(&want), std::sync::atomic::Ordering::SeqCst);
                    }
                    ST_PROBE.store(1, std::sync::atomic::Ordering::SeqCst);
                    Cow::Owned(Vec::new())
                }
                _ => {
                    let th = crate::theme::Theme::resolve(&crate::config::Config::load().theme);
                    Cow::Owned(th.apply_html(engine::HTML).into_bytes())
                }
            };
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html")
                .body(body)
                .unwrap()
        })
        .with_url("ralfred://index.html")
        .build(&window)
        .map_err(|e| format!("webview: {e}"))?;

    // global hotkey -> UserEvent(SHOW)
    if let Some(combo) = hotkey {
        let hk = crate::hotkey::parse_combo(combo)?;
        let mgr = global_hotkey::GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        mgr.register(hk)
            .map_err(|e| format!("register: {e} (accessibility / already taken?)"))?;
        let ev = global_hotkey::GlobalHotKeyEvent::receiver();
        let proxy: EventLoopProxy<UserEvent> = event_loop.create_proxy();
        // watch for user-changed hotkey (settings patch) and re-register live
        let mut cur = combo.to_string();
        let cur_id = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(hk.id));
        let id_r = cur_id.clone();
        let proxy_r = proxy.clone();
        std::thread::spawn(move || {
            let mut last_check = std::time::Instant::now();
            loop {
                match ev.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(e) if e.id() == id_r.load(std::sync::atomic::Ordering::Relaxed) => {
                        let _ = proxy_r.send_event(UserEvent(SHOW));
                    }
                    Ok(_) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
                if last_check.elapsed().as_secs_f64() < 1.0 {
                    continue;
                }
                last_check = std::time::Instant::now();
                let want = crate::config::Config::load().hotkey;
                if want != cur {
                    if let Ok(nhk) = crate::hotkey::parse_combo(&want) {
                        if mgr.unregister(hk).is_ok() && mgr.register(nhk).is_ok() {
                            cur = want;
                            id_r.store(nhk.id, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            }
        });
        let _ = proxy;
    }

    let mut shown = hotkey.is_none(); // dev mode: no hotkey -> start visible
    if shown {
        window.set_visible(true);
        window.set_focus();
    }

    // large-type handoff for the LT event consumer in the loop
    let shared_lt = shared.clone();

    // ---- selftest ----
    let selftest = std::env::var("RALFRED_SELFTEST").is_ok();
    let st_q = std::env::var("RALFRED_SELFTEST_Q").unwrap_or_else(|_| "2+2".into());
    let mut st_phase = 0u8;
    let mut st_t: Option<std::time::Instant> = None;
    let mut st_first_routed: Option<std::time::Instant> = None;
    let _ = &st_first_routed;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = if selftest {
            ControlFlow::Poll
        } else {
            ControlFlow::Wait
        };
        match event {
            Event::UserEvent(UserEvent(k)) if k == SHOW => {
                window.set_visible(true);
                window.set_focus();
                shown = true;
                let _ = webview.evaluate_script("rfReset && rfReset();");
            }
            Event::UserEvent(UserEvent(k)) if k == LT => {
                let text = shared_lt.lt.lock().unwrap().take();
                if let Some(t) = text {
                    window.set_fullscreen(Some(tao::window::Fullscreen::Borderless(None)));
                    window.set_visible(true);
                    window.set_focus();
                    let esc: String = t.chars().flat_map(|ch| match ch {
                        '\\' => "\\\\".chars().collect::<Vec<_>>(),
                        '\'' => vec!['\\', '\''],
                        '\n' => vec!['\\', 'n'],
                        '\r' => vec!['\\', 'r'],
                        c => vec![c],
                    }).collect();
                    let sz = crate::largetype::font_size(&t);
                    let _ = webview.evaluate_script(&format!("rfLargeType('{}',{});", esc, sz));
                }
            }
            Event::UserEvent(UserEvent(_)) => {
                let _ = webview.evaluate_script("rfLargeTypeOff && rfLargeTypeOff();");
                window.set_fullscreen(None);
                window.set_inner_size(tao::dpi::LogicalSize::new(680.0, 58.0));
                window.set_visible(false);
                shown = false;
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(false),
                window_id: id,
                ..
            } if window.id() == id && shown => {
                window.set_visible(false);
                shown = false;
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => {}
        }
        if selftest {
            let now = std::time::Instant::now();
            match st_phase {
                0 => {
                    let _t = (st_q.clone(), now);
                    let js = format!("rfSearch && rfSearch({:?});", st_q);
                    let _ = webview.evaluate_script(&js);
                    st_t = Some(now);
                    st_phase = 1;
                }
                1 => {
                    let n = shared.items.lock().unwrap().len();
                    if n > 0 && st_first_routed.is_none() {
                        st_first_routed = Some(now);
                        println!("SELFTEST routed {n} candidates");
                    }
                    if n == 0 {
                        let js = format!("rfSearch && rfSearch({:?});", st_q);
                        let _ = webview.evaluate_script(&js);
                        if st_t.map(|t| t.elapsed().as_secs() > 8).unwrap_or(false) {
                            println!("SELFTEST FAIL: search never routed");
                            *control_flow = ControlFlow::Exit;
                        }
                    } else if st_first_routed.is_some() {
                        // routed -> fire probe; watch DOM_ROWS (set by handler on probe arrival)
                        let _ = webview.evaluate_script(
                            "rfProbe && rfProbe(document.querySelectorAll('.i').length);");
                        if DOM_ROWS.swap(0, std::sync::atomic::Ordering::SeqCst) > 0 {
                            println!("SELFTEST dom_rows drawn");
                            // theme probe: JS compares served RF_BG to Rust-resolved theme
                            let _ = webview.evaluate_script("rfTheme && rfTheme();");
                            st_phase = 2;
                            st_t = Some(now);
                        } else if st_first_routed.map(|t| t.elapsed().as_secs() > 6).unwrap_or(false) {
                            println!("SELFTEST FAIL: dom never drew rows");
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                }
                2 => {
                    if THEME_OK.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        println!("SELFTEST theme ok");
                        st_phase = 3;
                        st_t = Some(now);
                    } else if st_t.map(|t| t.elapsed().as_secs() > 4).unwrap_or(false) {
                        println!("SELFTEST FAIL: theme probe never matched");
                        *control_flow = ControlFlow::Exit;
                    }
                }
                3 => {
                    if EXEC_ACK.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        println!("SELFTEST exec ok");
                        st_phase = 4; // Exit takes effect next poll; stop re-counting
                        *control_flow = ControlFlow::Exit;
                    } else {
                        let _ = webview.evaluate_script("rfExec && rfExec(0);");
                        if st_t.map(|t| t.elapsed().as_secs() > 6).unwrap_or(false) {
                            println!("SELFTEST FAIL: exec timeout");
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(0));
            i += 3;
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).to_string()
}
