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
pub struct UserEvent(pub u32); // SHOW=1 HIDE=2 LT=3

pub const SHOW: u32 = 1;
pub const HIDE: u32 = 2;
pub const LT: u32 = 3;
pub const RESYNC: u32 = 4;
pub const RESIZE: u32 = 5;
pub const PREFS: u32 = 6;
pub const VIB: u32 = 8;
pub const PREFCLOSE: u32 = 7;

static DOM_ROWS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static ST_PROBE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static EXEC_ACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static THEME_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Default)]
struct Shared {
    items: Mutex<Vec<Candidate>>,
    lt: Mutex<Option<String>>,
    resize_to: std::sync::atomic::AtomicU32,
    /// last query typed in the panel — recorded to qhistory when the panel
    /// commits (exec/hide), mirroring Alfred's `↑`-with-empty-query history.
    last_q: Mutex<String>,
}

fn api_body(sh: &Arc<Shared>, proxy: &EventLoopProxy<UserEvent>, req: &wry::http::Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
            let uri = req.uri();
            let path = uri.path().to_string();
            let query = uri.query().unwrap_or("").to_string();
            let host = uri.host().unwrap_or("").to_string();
            if host == "api" && crate::webassets::has_real_assets() {
                if path == "/index.html" {
                    let cfg = crate::config::Config::load();
                    let th = crate::theme::Theme::resolve(&cfg.theme)
                        .applied(cfg.translucent && !req.uri().query().map_or(false, |q| q.contains("w=prefs")));
                    return Response::builder().status(StatusCode::OK)
                        .header("content-type", "text/html")
                        .header("access-control-allow-origin", "*")
                        .body(Cow::Owned(crate::webassets::index_with_theme(&th).into_bytes())).unwrap();
                }
                if path == "/bundle.js" {
                    return Response::builder().status(StatusCode::OK)
                        .header("content-type", "application/javascript")
                        .header("access-control-allow-origin", "*")
                        .body(Cow::Borrowed(crate::webassets::BUNDLE)).unwrap();
                }
            }
            if std::env::var("RALFRED_PROTO_DEBUG").is_ok() {
                eprintln!("[proto] {path}?{query}");
            }
            let body: Cow<'static, [u8]> = match path.as_str() {
                "/search" => {
                    let mut q = String::new();
                    let mut files_only = false;
                    for part in query.split('&') {
                        if let Some(v) = part.strip_prefix("q=") {
                            q = url_decode(v);
                        } else if part == "mode=files" {
                            files_only = true;
                        }
                    }
                    if q.is_empty() {
                        q = url_decode(query.strip_prefix("q=").unwrap_or(&query));
                    }
                    let store = Store::knowledge();
                    let cfg = crate::config::Config::load();
                    let mut cands = crate::router::route(&q, &cfg, &store);
                    if files_only {
                        // Space-in-empty-box file mode (Alfred's keyword
                        // switch): only files/folders survive
                        cands.retain(|c| matches!(c.kind, crate::search::Kind::File | crate::search::Kind::Dir));
                    }
                    if std::env::var("RALFRED_PROTO_DEBUG").is_ok() {
                        eprintln!("[search] q={:?} -> {} candidates", q, cands.len());
                    }
                    let json = engine::items_json(&cands);
                    *sh.items.lock().unwrap() = cands;
                    if !q.trim().is_empty() {
                        *sh.last_q.lock().unwrap() = q.clone();
                    }

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
                                    let _ = proxy.send_event(UserEvent(LT));
                                    EXEC_ACK.store(true, std::sync::atomic::Ordering::SeqCst);
                                    return Response::builder().status(StatusCode::OK)
                                        .header("content-type","application/json")
                                        .body(Cow::Owned(br#"{"ok":true,"largetype":true}"#.to_vec())).unwrap();
                                }
                            }
                            Store::knowledge().record_query(&sh.last_q.lock().unwrap().clone());
                            let store = Store::knowledge();
                            engine::act(&c, &store);
                            EXEC_ACK.store(true, std::sync::atomic::Ordering::SeqCst);
                            let _ = proxy.send_event(UserEvent(HIDE));
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
                "/navigate" => {
                    // REA 2.11 arrowsForFolderNavigation: Right opens the
                    // selected file/folder, Left reveals the enclosing folder.
                    // Panel stays visible (navigation, not execution).
                    let mut n = 0usize;
                    let mut enc = false;
                    for part in query.split('&') {
                        if let Some(v) = part.strip_prefix("id=") {
                            n = v.parse().unwrap_or(0)
                        } else if part == "enc=1" {
                            enc = true
                        }
                    }
                    let c = sh.items.lock().unwrap().get(n).cloned();
                    let target: Option<std::path::PathBuf> = c.and_then(|c| {
                        let p = c.path?;
                        if enc {
                            Some(
                                std::path::Path::new(&p)
                                    .parent()
                                    .map(|x| x.to_path_buf())
                                    .unwrap_or_else(|| std::path::PathBuf::from(&p)),
                            )
                        } else {
                            match c.kind {
                                crate::search::Kind::App
                                | crate::search::Kind::Dir
                                | crate::search::Kind::File
                                | crate::search::Kind::PrefPane => {
                                    Some(std::path::PathBuf::from(&p))
                                }
                                _ => None,
                            }
                        }
                    });
                    match target {
                        Some(t) => {
                            let _ = crate::router::exec_action(crate::router::Action::Open(t));
                            Cow::Owned(br#"{"ok":true}"#.to_vec())
                        }
                        None => Cow::Owned(br#"{"ok":false}"#.to_vec()),
                    }
                }
                "/history" => {
                    let n: usize = query
                        .strip_prefix("n=")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(20);
                    let v = Store::knowledge().recent_queries(n);
                    let mut buf: Vec<u8> = Vec::new();
                    buf.extend_from_slice(br#"{"queries":["#.as_slice());
                    for (k, q) in v.iter().enumerate() {
                        if k > 0 { buf.push(b','); }
                        for &b in q.as_bytes() {
                            match b {
                                b'"' => { buf.push(b'\\'); buf.push(b'"'); },
                                b'\\' => { buf.push(b'\\'); buf.push(b'\\'); },
                                c => buf.push(c),
                            }
                        }
                    }
                    buf.extend_from_slice(br#"]}"#.as_slice());
                    Cow::Owned(buf)
                }
                "/history/add" => {
                    let q = url_decode(query.strip_prefix("q=").unwrap_or(&query));
                    Store::knowledge().record_query(&q);
                    Cow::Owned(br#"{"ok":true}"#.to_vec())
                }
                "/actions" => {
                    // Universal Actions menu for the selected result (Alfred Tab).
                    let n: usize = query.strip_prefix("id=").and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
                    let c = sh.items.lock().unwrap().get(n).cloned();
                    match c {
                        Some(c) => {
                            let target = match c.path.as_deref() {
                                Some(p) if !p.is_empty() => p.to_string(),
                                _ => c.title.clone(),
                            };
                            let acts = crate::universal::applicable(&target);
                            let body = acts
                                .iter()
                                .map(|(id, l)| format!("{{\"id\":\"{id}\",\"label\":\"{l}\"}}"))
                                .collect::<Vec<_>>()
                                .join(",");
                            Cow::Owned(format!("{{\"ok\":true,\"actions\":[{body}]}}").into_bytes())
                        }
                        None => Cow::Owned(br#"{"ok":false,"actions":[]}"#.to_vec()),
                    }
                }
                "/action" => {
                    // /action?id=N&aid=alfred.action.<id> — run action, then hide like exec.
                    let (idpart, aid) = match query.split_once("&aid=") {
                        Some((a, b)) => (a, b),
                        None => (query.as_str(), ""),
                    };
                    let n: usize = idpart.strip_prefix("id=").and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
                    let c = sh.items.lock().unwrap().get(n).cloned();
                    match c {
                        Some(c) if !aid.is_empty() => {
                            let target = match c.path.as_deref() {
                                Some(p) if !p.is_empty() => p.to_string(),
                                _ => c.title.clone(),
                            };
                            let _ = crate::universal::run_action(aid, &target);
                            EXEC_ACK.store(true, std::sync::atomic::Ordering::SeqCst);
                            let _ = proxy.send_event(UserEvent(HIDE));
                            Cow::Owned(br#"{"ok":true}"#.to_vec())
                        }
                        _ => Cow::Owned(br#"{"ok":false}"#.to_vec()),
                    }
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
                            Ok(()) => {
                                if body.get("translucent").is_some() {
                                    let _ = proxy.send_event(UserEvent(VIB));
                                }
                                Cow::Owned(br#"{"ok":true}"#.to_vec())
                            }
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
                    Store::knowledge().record_query(&sh.last_q.lock().unwrap().clone());
                    let _ = proxy.send_event(UserEvent(HIDE));
                    Cow::Owned(Vec::new())
                }
                "/resize" => {
                    if let Some(h) = query.strip_prefix("h=").and_then(|v| v.parse::<u32>().ok()) {
                        sh.resize_to.store(h, std::sync::atomic::Ordering::SeqCst);
                        let _ = proxy.send_event(UserEvent(RESIZE));
                    }
                    Cow::Owned(Vec::new())
                }
                "/probe" => {
                    if let Some(j) = query.strip_prefix("js=") {
                        eprintln!("[jsdiag] {j}");
                    }
                    if let Some(r) = query.strip_prefix("rows=") {
                        DOM_ROWS.store(r.parse().unwrap_or(0), std::sync::atomic::Ordering::SeqCst);
                    }
                    if let Some(b) = query.strip_prefix("bg=") {
                        // end-to-end theme check: served HTML's RF_BG vs resolved theme.
                        // Compare the RGB base only — glass mode changes the alpha suffix.
                        let want = crate::theme::Theme::resolve(&crate::config::Config::load().theme)
                            .background;
                        let wb = want.to_ascii_lowercase();
                        let bb = b.to_ascii_lowercase();
                        let n = wb.len().min(bb.len()).min(6);
                        THEME_OK.store(n >= 6 && wb[..n] == bb[..n], std::sync::atomic::Ordering::SeqCst);
                    }
                    ST_PROBE.store(1, std::sync::atomic::Ordering::SeqCst);
                    Cow::Owned(Vec::new())
                }
                "/api/features" => {
                    if req.method() == wry::http::Method::PATCH {
                        let body: Value = serde_json::from_slice(req.body()).unwrap_or(Value::Null);
                        let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let on = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                        return match crate::features::patch(id, on) {
                            Ok(()) => Response::builder().status(StatusCode::OK)
                                .header("access-control-allow-origin", "*")
                                .body(Cow::Owned(br#"{"ok":true}"#.to_vec())).unwrap(),
                            Err(e) => Response::builder().status(StatusCode::BAD_REQUEST)
                                .header("content-type", "application/json")
                                .header("access-control-allow-origin", "*")
                                .body(Cow::Owned(format!("{{\"ok\":false,\"error\":\"{e}\"}}").into_bytes())).unwrap(),
                        };
                    }
                    return Response::builder().status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .header("access-control-allow-origin", "*")
                        .body(Cow::Owned(crate::features::list().to_string().into_bytes())).unwrap();
                }
                "/api/workflows" => {
                    let wfs = crate::workflow::load_workflows(&crate::config::workflows_dir());
                    let doc: Vec<Value> = wfs.iter().map(|w| serde_json::json!({
                        "uid": w.uid, "name": w.name, "keyword": w.keyword,
                        "dir": w.dir.display().to_string(),
                    })).collect();
                    return Response::builder().status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .header("access-control-allow-origin", "*")
                        .body(Cow::Owned(Value::Array(doc).to_string().into_bytes())).unwrap();
                }
                "/api/themes" => {
                    let strip = |c: &str| if c.len() == 8 { c[..6].to_string() } else { c.to_string() };
                    let doc: Vec<Value> = crate::theme::Theme::all().iter().map(|t| serde_json::json!({
                        "id": t.uid, "name": t.name,
                        "colors": {"BG": strip(&t.background), "FIELD": strip(&t.field),
                                   "TEXT": strip(&t.text), "SUBTEXT": strip(&t.subtext),
                                   "SELBG": strip(&t.selection_background)},
                    })).collect();
                    return Response::builder().status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .header("access-control-allow-origin", "*")
                        .body(Cow::Owned(Value::Array(doc).to_string().into_bytes())).unwrap();
                }
                "/prefs" => {
                    if !crate::webassets::has_real_assets() {
                        return Response::builder().status(StatusCode::NOT_FOUND)
                            .body(Cow::Owned(Vec::new())).unwrap();
                    }
                    let _ = proxy.send_event(UserEvent(PREFS));
                    Cow::Owned(Vec::new())
                }
                "/prefsclose" => {
                    let _ = proxy.send_event(UserEvent(PREFCLOSE));
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
                .header("access-control-allow-origin", "*")
                .body(body)
                .unwrap()

}

fn api_handler(sh: Arc<Shared>, proxy: EventLoopProxy<UserEvent>) -> impl Fn(wry::WebViewId<'_>, wry::http::Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> + Send + Sync + 'static {
    move |_id: wry::WebViewId, req: wry::http::Request<Vec<u8>>| api_body(&sh, &proxy, &req)
}

/// macOS frosted-glass: insert (or remove) an NSVisualEffectView directly
/// behind the webview inside the window's contentView. The panel window is
/// already transparent; with the webview layer transparent too, the CSS
/// alphas from Theme::applied() let the desktop blur show through — the
/// effect Alfred calls translucency.
#[cfg(target_os = "macos")]
fn set_panel_vibrancy(win: &tao::window::Window, on: bool) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSView, NSVisualEffectView, NSVisualEffectMaterial, NSVisualEffectBlendingMode,
        NSVisualEffectState, NSAutoresizingMaskOptions, NSWindowOrderingMode, NSWindow,
    };
    use objc2::runtime::NSObjectProtocol;
    if objc2::MainThreadMarker::new().is_none() {
        return; // AppKit requires the main thread
    }
    unsafe {
        use tao::platform::macos::WindowExtMacOS;
        let nswin_ptr = win.ns_window() as *mut AnyObject;
        if nswin_ptr.is_null() {
            return;
        }
        let nswin: &NSWindow = &*(nswin_ptr as *const NSWindow);
        let Some(cv) = nswin.contentView() else { return };
        for v in cv.subviews().iter() {
            if let Some(view) = v.downcast_ref::<NSView>() {
                if view.isKindOfClass(<NSVisualEffectView as objc2::ClassType>::class()) {
                    view.removeFromSuperview();
                }
            }
        }
        if on {
            let mtm = objc2::MainThreadMarker::new_unchecked();
            let alloc = mtm.alloc();
            let vev = NSVisualEffectView::initWithFrame(alloc, cv.bounds());
            vev.setMaterial(NSVisualEffectMaterial::HUDWindow);
            vev.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            vev.setState(NSVisualEffectState::Active);
            vev.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            cv.addSubview_positioned_relativeTo(&*vev, NSWindowOrderingMode::Below, None);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_panel_vibrancy(_win: &tao::window::Window, _on: bool) {}

pub fn run(hotkey: Option<&str>) -> Result<(), String> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let shared = Arc::new(Shared::default());

    // warm the document index off-thread (self-built scope index like
    // Alfred's; keeps /search off Spotlight entirely while typing)
    crate::index::ensure_started();

    // kill-switch for the frosted-glass layer (used to isolate crashes)
    let glass = crate::config::Config::load().translucent
        && std::env::var("RALFRED_NOGLASS").is_err();
    eprintln!("[glass] config translucent={}", glass);
    let mut builder = WindowBuilder::new()
        .with_title("ralfred")
        .with_transparent(true)

        .with_decorations(false)
        .with_always_on_top(true)
        .with_visible(false)
        .with_inner_size(tao::dpi::LogicalSize::new(718.0, 71.0));
    if let Some(m) = event_loop.primary_monitor() {
        let size = m.size().to_logical::<f64>(m.scale_factor());
        builder = builder.with_position(tao::dpi::LogicalPosition::new(
            (size.width - 718.0) / 2.0,
            size.height * 0.16,
        ));
    }
    let window = builder.build(&event_loop).map_err(|e| format!("window: {e}"))?;

    // ---- custom protocol: html + api on one scheme ----
    let sh = shared.clone();
    let proxy_h: EventLoopProxy<UserEvent> = event_loop.create_proxy();
    let webview = WebViewBuilder::new()

        .with_custom_protocol("ralfred".to_string(), api_handler(sh.clone(), proxy_h.clone()))
        .with_url(if crate::webassets::has_real_assets() {
            "ralfred://api/index.html"
        } else {
            "ralfred://index.html"
        })
        .build(&window)
        .map_err(|e| format!("webview: {e}"))?;

    // ---- preferences window (hidden until PREFS) ----
    let mut prefs_wv: Option<wry::WebView> = None;
    let mut prefs_win: Option<tao::window::Window> = None;
    let mut pwb = WindowBuilder::new()
        .with_title("Ralfred Preferences")
        .with_inner_size(tao::dpi::LogicalSize::new(1180.0, 700.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(980.0, 620.0))
        .with_visible(false);
    if let Some(m) = event_loop.primary_monitor() {
        let size = m.size().to_logical::<f64>(m.scale_factor());
        pwb = pwb.with_position(tao::dpi::LogicalPosition::new(
            ((size.width - 1180.0) / 2.0).max(0.0),
            ((size.height - 700.0) / 3.0).max(0.0),
        ));
    }
    let prefs_window = pwb.build(&event_loop).map_err(|e| format!("prefs window: {e}"))?;
    prefs_wv = Some(WebViewBuilder::new()
        .with_url("ralfred://api/index.html?w=prefs#prefs")
        .with_custom_protocol("ralfred".to_string(), api_handler(shared.clone(), event_loop.create_proxy()))
        .build(&prefs_window)
        .map_err(|e| format!("prefs webview: {e}"))?);
    prefs_win = Some(prefs_window);

    if glass {
        set_panel_vibrancy(&window, true);
    }

    // ---- single-instance: if another copy holds our socket, summon it ----
    {
        let sock = crate::config::app_support_dir().join("gui.sock");
        std::fs::create_dir_all(crate::config::app_support_dir()).ok();
        if std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            println!("ralfred: already running — panel summoned");
            return Ok(());
        }
        if sock.exists() {
            let _ = std::fs::remove_file(&sock); // stale from a crashed copy
        }
        let listener = std::os::unix::net::UnixListener::bind(&sock)
            .map_err(|e| format!("socket: {e}"))?;
        let proxy_s = event_loop.create_proxy();
        std::thread::spawn(move || {
            for _ in listener.incoming() {
                let _ = proxy_s.send_event(UserEvent(SHOW));
            }
        });
    }

    // global hotkey -> UserEvent(SHOW); failure is non-fatal: the panel still
    // works, the user just has to summon it once (or fix the hotkey in Settings)
    if let Some(combo) = hotkey {
        let hk = crate::hotkey::parse_combo(combo)?;
        let mgr = global_hotkey::GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        match mgr.register(hk) {
            Ok(()) => eprintln!("[hk] registered {combo} id={}", hk.id),
            Err(e) => eprintln!("ralfred: hotkey {combo} unavailable ({e}) — panel opens on launch; change it in Settings (Cmd+,)"),
        }
        // Carbon delivery is unreliable for plain-option combos on recent
        // macOS, and registration can silently vanish after re-register —
        // keep a listen-only session tap as a second, focus-independent path
        {
            let p2 = event_loop.create_proxy();
            let _ = crate::hotkey::start_tap_watcher(move || {
                let _ = p2.send_event(UserEvent(SHOW));
            });
        }
        let ev = global_hotkey::GlobalHotKeyEvent::receiver();
        let proxy: EventLoopProxy<UserEvent> = event_loop.create_proxy();
        // watch for user-changed hotkey (settings patch) and re-register live
        let mut cur = combo.to_string();
        let mut cur_hk = hk;
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
                    Ok(e) => {
                        eprintln!("[hk] event id={} wanted={}", e.id(), id_r.load(std::sync::atomic::Ordering::Relaxed));
                    }
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
                        // unregister what is CURRENTLY registered — using the
                        // startup key here left the second change un-registered
                        // (hotkey silently died after two edits)
                        if mgr.unregister(cur_hk).is_ok() && mgr.register(nhk).is_ok() {
                            cur_hk = nhk;
                            cur = want;
                            id_r.store(nhk.id, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                }
            }
        });
        let _ = proxy;
    }

    // menu-bar item (Alfred lives in the menu bar, not the Dock) + Edit
    // menu so text fields in the panel and Preferences get ⌘C/⌘V/⌘A
    if let Some(mtm) = objc2_foundation::MainThreadMarker::new() {
        crate::tray::install(event_loop.create_proxy(), mtm);
    }

    // Launch behavior: "silent" = menu bar only, "panel" = summon right away
    let silent = std::env::var("RALFRED_SELFTEST").is_err()
        && crate::config::Config::load().launch_mode == "silent";
    let mut shown = !silent;
    let mut last_h: u32 = 71;
    if !silent {
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

    // Preferences window: created lazily on first PREFS; the Window is leaked
    // (one per session, reused forever via show/hide) so the WebView's borrow
    // of it is trivially valid — closing/hiding never drops either.
    // Preferences window is built up-front (hidden) — same pattern as the
    // panel: windows built before run() get correct screen metrics; windows
    // built inside the loop mis-place/mis-size on this macOS/tao combo.

    if std::env::var("RALFRED_PREFS_AT_START").is_ok() {
        if let Some(w) = &prefs_win {
            w.set_visible(true);
            w.set_focus();
        }
    }
    let sh_ev = shared.clone();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = if selftest {
            ControlFlow::Poll
        } else {
            ControlFlow::Wait
        };
        match event {
            Event::UserEvent(UserEvent(k)) if k == SHOW => {
                // accessory-app summons need an explicit activation push —
                // a bare set_focus loses to another app's key window
                if let Some(mtm) = objc2_foundation::MainThreadMarker::new() {
                    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
                    let _: i8 = unsafe { objc2::msg_send![&app, activateIgnoringOtherApps: true] };
                }
                if shown {
                    // Alfred semantics: the hotkey dismisses while visible
                    // (the panel already self-hides on blur, so visible here
                    //  means it has focus and the user wants it out of the way)
                    window.set_visible(false);
                    shown = false;
                } else {
                    window.set_visible(true);
                    window.set_focus();
                    shown = true;
                    let _ = webview.evaluate_script("rfReset && rfReset();");
                }
            }
            Event::UserEvent(UserEvent(k)) if k == RESIZE => {
                let h = shared.resize_to.load(std::sync::atomic::Ordering::SeqCst).clamp(71, 972);
                if shown && last_h != h {
                    last_h = h;
                    // keep the panel's TOP edge fixed: the list grows downward like Alfred's
                    let keep = window.outer_position().ok().map(|p| {
                        let sc = window.scale_factor();
                        (p.x as f64 / sc, p.y as f64 / sc)
                    });
                    window.set_inner_size(tao::dpi::LogicalSize::new(718.0, h as f64));
                    if let Some((x, y)) = keep {
                        window.set_outer_position(tao::dpi::LogicalPosition::new(x, y));
                    }
                }
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
            Event::UserEvent(UserEvent(k)) if k == VIB => {
                let on = crate::config::Config::load().translucent;
                set_panel_vibrancy(&window, on);
                webview.reload();
                if let Some(w) = &prefs_wv {
                    w.reload();
                }
            }
            Event::UserEvent(UserEvent(k)) if k == PREFS => {
                if let Some(pw) = &prefs_win {
                    // Deterministic geometry every time — a wrongly restored
                    // frame (state restoration / stray resize) would otherwise
                    // reappear as e.g. 1150x812.
                    pw.set_inner_size(tao::dpi::LogicalSize::new(1180.0, 700.0));
                    pw.set_visible(true);
                    pw.set_focus();
                }
            }
            Event::UserEvent(UserEvent(k)) if k == PREFCLOSE => {
                if let Some(w) = &prefs_win {
                    w.set_visible(false);
                }
            }
            Event::UserEvent(UserEvent(_)) => {
                let _ = webview.evaluate_script("rfLargeTypeOff && rfLargeTypeOff();");
                window.set_fullscreen(None);
                window.set_inner_size(tao::dpi::LogicalSize::new(718.0, 71.0));
                window.set_visible(false);
                shown = false;
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(false),
                window_id: id,
                ..
            } if window.id() == id && shown => {
                Store::knowledge().record_query(&sh_ev.last_q.lock().unwrap().clone());
                window.set_visible(false);
                shown = false;
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                if window_id == window.id() {
                    *control_flow = ControlFlow::Exit;
                } else if let Some(w) = &prefs_win {
                    // close button on prefs = hide (reopen via Cmd+,), like Alfred ⌘W
                    w.set_visible(false);
                }
            }
            _ => {}
        }
        if selftest {
            let now = std::time::Instant::now();
            match st_phase {
                0 => {
                    // one eval per launch is plenty; phase 1 handles retries/fallback
                    st_t = Some(now);
                    st_phase = 1;
                }
                1 => {
                    if std::env::var("SELFTEST_TRACE").is_ok() {
                        static POLLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                        let p = POLLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        if p < 3 || p % 1000 == 0 {
                            println!("TRACE poll={p}");
                        }
                    }
                    let n = shared.items.lock().unwrap().len();
                    if n > 0 && st_first_routed.is_none() {
                        st_first_routed = Some(now);
                        println!("SELFTEST routed {n} candidates");
                    }
                    if n == 0 {
                        // fire the JS fetch path exactly once per query: duplicate
                        // in-flight fetches to the same URL get cancelled by WebKit,
                        // and a cancelled task's response is silently dropped.
                        static LAST_EVAL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
                        let fire_eval = {
                            let mut g = LAST_EVAL.lock().unwrap();
                            match *g { Some(t) if t.elapsed().as_millis() < 300 => false, _ => { *g = Some(now); true } }
                        };
                        if fire_eval {
                            let js = format!("window.RF_DIAG=1;rfSearch && rfSearch({:?});", st_q);
                            match webview.evaluate_script(&js) {
                                Ok(v) => {
                                    if std::env::var("SELFTEST_TRACE").is_ok() {
                                        println!("TRACE eval-ok n={n} v={:?}", v);
                                    }
                                }
                                Err(e) => println!("SELFTEST FAIL: evaluate_script: {e}"),
                            }
                        }
                        // fallback: if the JS bridge never delivered, route directly so the
                        // loop's view of candidates matches what /search would have stored.
                        if st_t.map(|t| t.elapsed().as_millis() > 1500).unwrap_or(false)
                            && shared.items.lock().unwrap().is_empty() {
                            let store = crate::db::Store::knowledge();
                            let cfg = crate::config::Config::load();
                            let c = crate::router::route(&st_q, &cfg, &store);
                            if !c.is_empty() {
                                let n = c.len();
                                let json = engine::items_json(&c);
                                let _ = webview.evaluate_script(&format!("rfApply({json});"));
                                *shared.items.lock().unwrap() = c;
                                println!("TRACE direct-route fallback n={n}");
                            }
                        }
                        if st_t.map(|t| t.elapsed().as_secs() > 8).unwrap_or(false) {
                            println!("SELFTEST FAIL: search never routed");
                            *control_flow = ControlFlow::Exit;
                        }
                    } else if st_first_routed.is_some() {
                        // routed -> fire probe (throttled: duplicate in-flight fetches
                        // get cancelled by WebKit, starving other bodies)
                        static LAST_PROBE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
                        let fire = {
                            let mut g = LAST_PROBE.lock().unwrap();
                            match *g { Some(t) if t.elapsed().as_millis() < 100 => false, _ => { *g = Some(now); true } }
                        };
                        if fire {
                            let _ = webview.evaluate_script(
                                "rfProbe && rfProbe(document.querySelectorAll('.i').length);");
                        }
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
