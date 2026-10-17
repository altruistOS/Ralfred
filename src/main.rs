//! ralfred — Alfred-style launcher (clean-room, Rust).
//!
//! Modes:
//!   ralfred                       interactive TUI (search box + ranked results)
//!   ralfred <query>               one-shot; prints tab-separated results
//!   ralfred <query> --open        act on the first result
//!   ralfred snippet <kw> <text>   add snippet (keyword auto-expand)
//!   ralfred clipboard [q...]      list/search clipboard history; --push reads stdin
//!   ralfred calc <expr>           evaluate calculator expression
//!   ralfred workflows             list installed workflows
//!   ralfred config                show resolved config

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, List, ListState, Paragraph},
    Terminal,
};
use ralfred::{
    calc,
    db::Store,
    router::{self, Action},
    search::{Candidate, Kind},
    Config,
};
use std::io::{self, Read, Stdout, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        None => {
            // double-clicked from Finder (inside an .app bundle): no args,
            // no TTY — TUI would fail with "Device not configured". Launch
            // the panel with the default hotkey instead.
            if std::env::current_exe()
                .map(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
                .unwrap_or(false)
            {
                let hk = ralfred::config::Config::load().hotkey;
                match ralfred::gui::run(Some(&hk)) {
                    Ok(()) => {}
                    Err(e) => eprintln!("gui error: {e}"),
                }
            } else {
                run_tui();
            }
        }
        // "search" is the documented query verb; strip it like the other
        // commands (the catch-all treats args[0] as the query itself)
        Some("search") | Some("find") => {
            let open_first = args.iter().any(|s| s == "--open");
            let query = args[1..]
                .iter()
                .filter(|a| *a != "--open")
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if query.is_empty() {
                println!("usage: ralfred search <query>");
                std::process::exit(1);
            }
            run_cli_search(&query, open_first);
        }
        Some("snippet") => cli_snippet(&args[1..]),
        Some("clipboard") => cli_clipboard(&args[1..]),
        Some("workflows") => cli_workflows(),
        Some("daemon") => cli_daemon(),
        Some("gui") => {
            let hk_cli = args.get(1).cloned();
            let hk = hk_cli.as_deref()
                .or_else(|| {
                    let c = ralfred::config::Config::load();
                    if c.hotkey != "ALT+CTRL+SPACE" {
                        // stored hotkey is explicit (non-default) -> honor it
                        return None;
                    }
                    None
                })
                .map(|s| s.to_string());
            let hk = hk.or_else(|| {
                let c = ralfred::config::Config::load();
                Some(c.hotkey.clone())
            });
            match ralfred::gui::run(hk.as_deref()) {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("gui error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some("theme") => ralfred::theme::cli(&args[1..]),
        Some("expand") => cli_expand(&args[1..]),
        Some("hotkey") => cli_hotkey(&args[1..]),
        Some("actions") => cli_actions(&args[1..]),
        Some("ql") => ralfred::quicklook::cli(&args[1..]),
        Some("icon") => ralfred::icons::cli(&args[1..]),
        Some("install") => ralfred::install::cli(&args[1..]),
        Some("migrate") => cli_migrate(&args[1..]),
        Some("config") => cli_config(),
        Some("calc") => {
            let expr = args[1..].join(" ");
            match calc::calculate(&expr) {
                Some(r) => println!("{r}"),
                None => {
                    eprintln!("invalid expression: {expr}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            if args.iter().any(|s| s == "--help" || s == "-h") {
                print_usage();
                return;
            }
            if args.iter().any(|s| s == "--version" || s == "-V") {
                println!("ralfred {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            let open_first = args.iter().any(|s| s == "--open");
            // An unrecognised long option in the query position must not be
            // searched for: `ralfred --gui` used to become a web search for
            // "--gui" instead of reporting the bad flag (SPEC §8.22).
            if let Some(bad) = args
                .iter()
                .find(|a| a.starts_with("--") && !matches!(a.as_str(), "--open"))
            {
                eprintln!("ralfred: unknown option '{bad}'");
                eprintln!("Try 'ralfred --help' for usage.");
                std::process::exit(64);
            }
            let query = args.iter().filter(|a| *a != "--open").cloned().collect::<Vec<_>>().join(" ");
            run_cli_search(&query, open_first);
        }
    }
}

fn print_usage() {
    println!(
        "ralfred {version} — an Alfred-style macOS launcher\n\
         \n\
         USAGE:\n\
         \x20 ralfred [<query>] [--open]   one-shot search (opens the top hit with --open)\n\
         \x20 ralfred                       interactive terminal UI\n\
         \x20 ralfred search <text>         default-results search\n\
         \x20 ralfred calc <expr>           unit-aware calculator\n\
         \x20 ralfred gui [<hotkey>]        native panel with a global hotkey\n\
         \x20 ralfred snippet <kw> [<text>] list or add snippets\n\
         \x20 ralfred expand               list auto-expanding snippets\n\
         \x20 ralfred clipboard [--push]    clipboard history (read stdin with --push)\n\
         \x20 ralfred actions [<verb>]      Universal Actions\n\
         \x20 ralfred theme [<verb>]        list / apply themes\n\
         \x20 ralfred ql | icon <path>      Quick Look preview / icon raster\n\
         \x20 ralfred hotkey [<verb>]       hotkey helpers\n\
         \x20 ralfred migrate [--dry-run]   import from Alfred 5 (read-only)\n\
         \x20 ralfred install [uninstall|status]\n\
         \x20 ralfred config               show the effective configuration\n\
         \x20 ralfred --help | --version\n\
         \n\
         Data lives in ~/Library/Application Support/ralfred (override with RALFRED_HOME).",
        version = env!("CARGO_PKG_VERSION")
    );
}

fn run_cli_search(query: &str, open_first: bool) {
    let store = Store::knowledge();
    let cfg = Config::load();
    let mut results = router::route(query, &cfg, &store);
    if results.is_empty() {
        results = router::path_candidates(query);
    }
    if results.is_empty() {
        std::process::exit(2);
    }
    for (i, r) in results.iter().enumerate() {
        let act = r.action.as_deref().unwrap_or("");
        let kind_str = if act.is_empty() { r.kind.label().to_string() } else { format!("{}|{}", r.kind.label(), act) };
        println!("{}\t{}\t{}\t{}", i, r.title, r.subtitle, kind_str);
    }
    if open_first {
        let first = results[0].clone();
        act(&first, &store);
    }
}

fn act(r: &Candidate, store: &Store) {
    if let Some(a) = &r.action {
        // workflow action tag: "<type>:<target>"
        let (typ, target) = a.split_once(':').unwrap_or((a.as_str(), ""));
        if typ.contains("openurl") {
            let _ = exec_action(Action::OpenUrl(target.to_string()));
            return;
        }
        if typ.contains("openfile") {
            if target.starts_with("http") {
                let _ = exec_action(Action::OpenUrl(target.to_string()));
            } else {
                let _ = exec_action(Action::Open(target.into()));
            }
            return;
        }
        if typ.contains("revealfile") {
            let _ = exec_action(Action::Reveal(target.into()));
            return;
        }
        if typ.contains("output.clipboard") {
            let _ = exec_action(Action::CopyText(target.to_string()));
            return;
        }
        if typ.contains("terminalcommand") || typ.contains("systemcommand") {
            // systemcommand -> sh -c; terminalcommand -> Terminal.app (evidence types)
            if typ.contains("systemcommand") {
                let _ = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(target)
                    .current_dir(std::env::temp_dir())
                    .spawn();
            } else {
                let _ = exec_action(Action::OpenTerminal(target.into()));
            }
            return;
        }
        if typ.starts_with("largetype") {
            let _ = ralfred::largetype::run(a);
            return;
        }
        if typ.starts_with("onepassword") {
            let _ = ralfred::onepassword::run(a);
            return;
        }
        if typ.starts_with("music") {
            let _ = ralfred::itunes::run(a);
            return;
        }
        if typ.contains("action.script") {
            let _ = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(target)
                .spawn();
            return;
        }
    }
    match r.kind {
        Kind::App | Kind::Dir | Kind::File | Kind::PrefPane => {
            if let Some(p) = &r.path {
                let _ = exec_action(Action::Open(p.into()));
                if let Some(path) = &r.path {
                    store.record_use(path, "");
                }
            }
        }
        Kind::WebSearch => {
            if let Some(u) = &r.path {
                if u.starts_with("http") {
                    let _ = exec_action(Action::OpenUrl(u.clone()));
                    store.record_use(u, "");
                } else if let Some(p) = &r.path {
                    // template keyword — copy url template, harmless
                    let _ = exec_action(Action::CopyText(p.clone()));
                }
            }
        }
        Kind::Snippet => {
            if let Some(t) = &r.path {
                let _ = exec_action(Action::SnippetText(t.clone()));
            }
        }
        Kind::Dictionary => {
            if let Some(u) = &r.path {
                let _ = exec_action(Action::OpenUrl(u.clone()));
            }
        }
        Kind::Clipboard => {
            if let Some(t) = &r.path {
                let _ = exec_action(Action::Paste(t.clone()));
            }
        }
        Kind::Calc => {
            let _ = exec_action(Action::CopyText(r.title.clone()));
        }
        _ => {
            if let Some(p) = &r.path {
                if p.starts_with("http") {
                    let _ = exec_action(Action::OpenUrl(p.clone()));
                } else if std::path::Path::new(p).exists() {
                    let _ = exec_action(Action::Open(p.into()));
                } else {
                    let _ = exec_action(Action::CopyText(p.clone()));
                }
            }
        }
    }
}

fn exec_action(a: Action) -> io::Result<()> {
    router::exec_action(a)
}

fn run_tui() {
    let store = Store::knowledge();
    let cfg = Config::load();
    let mut query = String::new();
    let mut results: Vec<Candidate> = Vec::new();
    let mut state = ListState::default();
    state.select(Some(0));

    let mut term = match setup_terminal() {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "ralfred: cannot start the interactive UI ({e}).\n\
                 This needs a terminal on stdout; try `ralfred <query>` or `ralfred --help`."
            );
            std::process::exit(1);
        }
    };
    let res = run_loop(&mut term, &mut query, &mut results, &mut state, &cfg, &store);
    teardown_terminal();
    if let Err(e) = res {
        eprintln!("error: {e}");
    }
}

fn run_loop(
    term: &mut Terminal<CrosstermBackend<Stdout>>,
    query: &mut String,
    results: &mut Vec<Candidate>,
    state: &mut ListState,
    cfg: &Config,
    store: &Store,
) -> io::Result<()> {
    loop {
        term.draw(|f| {
            let [top, bottom] = Layout::vertical([Constraint::Length(3), Constraint::Min(1)])
                .areas(f.area());
            f.render_widget(
                Paragraph::new(Line::from(format!("❯ {}", query))).block(Block::bordered().title(" ralfred ")),
                top,
            );
            let items: Vec<Line> = results
                .iter()
                .map(|c| {
                    Line::from(vec![
                        c.title.clone().white().bold().into(),
                        "  ".into(),
                        c.kind.label().dark_gray().into(),
                        " · ".into(),
                        c.subtitle.clone().dark_gray().into(),
                    ])
                })
                .collect();
            let list = List::new(items)
                .highlight_style(Style::new().bold().bg(ratatui::style::Color::DarkGray));
            f.render_stateful_widget(list, bottom, state);
        })?;

        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match k.code {
                KeyCode::Esc => return Ok(()),
                KeyCode::Down => {
                    let n = results.len().saturating_sub(1);
                    let i = (state.selected().unwrap_or(0) + 1).min(n);
                    state.select(Some(i));
                }
                KeyCode::Up => {
                    let i = state.selected().unwrap_or(0);
                    state.select(Some(i.saturating_sub(1)));
                }
                KeyCode::Enter => {
                    if let Some(r) = results.get(state.selected().unwrap_or(0)).cloned() {
                        act(&r, store);
                        return Ok(());
                    } else if !query.is_empty() {
                        let u = query.trim();
                        if u.starts_with("http") {
                            let _ = exec_action(Action::OpenUrl(u.to_string()));
                        } else {
                            let _ = exec_action(Action::OpenUrl(format!("https://www.google.com/search?q={}", urlenc(u))));
                        }
                        return Ok(());
                    }
                }
                KeyCode::Backspace => {
                    query.pop();
                    *results = compute_results(query, cfg);
                    state.select(Some(0));
                }
                KeyCode::Char(c) => {
                    if k.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                        return Ok(());
                    }
                    query.push(c);
                    *results = compute_results(query, cfg);
                    state.select(Some(0));
                }
                _ => {}
            }
        }
    }
}

fn urlenc(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn compute_results(query: &str, cfg: &Config) -> Vec<Candidate> {
    let store = Store::knowledge();
    let mut r = router::route(query, cfg, &store);
    if r.is_empty() && (query.starts_with('/') || query.starts_with("~/")) {
        r = router::path_candidates(query);
    }
    r
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut term = Terminal::new(backend)?;
    term.hide_cursor()?;
    Ok(term)
}

fn teardown_terminal() {
    disable_raw_mode().ok();
    execute!(io::stdout(), LeaveAlternateScreen).ok();
    io::stdout().flush().ok();
}

fn cli_snippet(args: &[String]) {
    if args.len() < 2 {
        eprintln!("usage: ralfred snippet <keyword> <text>");
        std::process::exit(1);
    }
    let store = Store::snippets();
    let body = args[1..].join(" ");
    let uid = store.add_snippet(&args[0], &body, &args[0]);
    println!("saved {uid}: {} -> {}", args[0], body);
}

fn cli_clipboard(args: &[String]) {
    let store = Store::clipboard();
    if args.iter().any(|a| a == "--push") {
        let mut s = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut s) {
            eprintln!("could not read the clipboard payload from stdin: {e}");
            std::process::exit(1);
        }
        store.clipboard_push(s.trim(), "ralfred");
        println!("pushed {} chars", s.trim().len());
        return;
    }
    let q = args.join(" ");
    let items = if q.is_empty() {
        store.clipboard_recent(20)
    } else {
        store.clipboard_search(&q, 20)
    };
    for (item, ts) in items {
        let h = ((ralfred::db::now_ts() - ts) / 3600.0).max(0.0) as u64;
        let first: String = item.lines().next().unwrap_or("").chars().take(80).collect();
        println!("{}h\t{}\t({} bytes)", h, first, item.len());
    }
}

fn cli_daemon() {
    // Clipboard history daemon (Alfred ClipboardManager poll model).
    // Entries are stored IMMEDIATELY on change (test-visible, crash-safe);
    // the frontmost-app attribution is resolved asynchronously and applied
    // via UPDATE, so a blocked/permission-denied probe never delays history.
    let store = Store::clipboard();
    let mut last: Option<String> = None;
    let mut pending: Option<(std::process::Child, i64, usize)> = None; // child,rowid,polls
    eprintln!("ralfred daemon: watching clipboard (Ctrl-C to stop)");
    loop {
        if let Some((mut ch, rowid, polls)) = pending.take() {
            match ch.try_wait() {
                Ok(Some(_)) => {
                    let app = ch.wait_with_output().ok()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_default();
                    if !app.is_empty() {
                        store.clipboard_set_app(rowid, &app);
                    }
                }
                Ok(None) if polls > 4 => {
                    let _ = ch.kill(); // System Events blocked — keep empty attribution
                }
                Ok(None) => pending = Some((ch, rowid, polls + 1)),
                Err(_) => {}
            }
        }
        if let Ok(o) = std::process::Command::new("pbpaste").output() {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            if !text.is_empty() && last.as_ref() != Some(&text) {
                let rowid = store.clipboard_push_now(&text);
                last = Some(text);
                if rowid > 0 {
                    if let Some((mut old, _, _)) = pending.take() {
                        let _ = old.kill();
                    }
                    if let Ok(c) = std::process::Command::new("osascript")
                        .arg("-e")
                        .arg("tell application \"System Events\" to get name of first application process whose frontmost is true")
                        .stderr(std::process::Stdio::null())
                        .spawn()
                    {
                        pending = Some((c, rowid, 0));
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}
fn cli_workflows() {
    let wfs = ralfred::workflow::load_workflows(&ralfred::config::workflows_dir());
    println!("{} workflows in {}", wfs.len(), ralfred::config::workflows_dir().display());
    for w in wfs {
        println!("- {:?} keyword={:?} objects={}", w.name, w.keyword, w.objects.len());
    }
}

fn cli_config() {
    let cfg = Config::load();
    println!("hotkey:      {}", cfg.hotkey);
    println!("theme:       {}", cfg.theme);
    println!("scope:       {:?}", cfg.default_scope);
    println!("max_results: {}", cfg.max_results);
    println!("web_search:  {:?}", cfg.web_search);
    println!("data_dir:    {}", ralfred::config::data_dir().display());
    println!("workflows:   {}", ralfred::config::workflows_dir().display());
}

fn cli_actions(args: &[String]) {
    let q = args.join(" ");
    if q.is_empty() {
        for a in ralfred::universal::known_actions() {
            println!("{a}");
        }
        return;
    }
    // "<action-id> <target>" e.g. `ralfred actions alfred.action.copypath /tmp/x`
    let (id, target) = q.split_once(' ').unwrap_or((q.as_str(), ""));
    match ralfred::universal::run_action(id, target) {
        Ok(()) => println!("done"),
        Err(e) => {
            eprintln!("action failed: {e}");
            std::process::exit(1);
        }
    }
}

fn cli_migrate(args: &[String]) {
    let dry = args.iter().any(|a| a == "--dry-run");
    let rep = ralfred::migration::migrate(dry);
    println!(
        "workflows: copied={} skipped={}  db_rows={}  errors={}",
        rep.workflows_copied, rep.workflows_skipped, rep.db_rows_imported, rep.errors.len()
    );
    for e in rep.errors {
        eprintln!("  ! {e}");
    }
}

fn cli_hotkey(args: &[String]) {
    match args.first().map(|s| s.as_str()) {
        None => {
            eprintln!("usage: ralfred hotkey <combo>   e.g. ALT+CTRL+SPACE");
            eprintln!("       ralfred hotkey list");
            std::process::exit(1);
        }
        Some("list") => ralfred::hotkey::list(),
        Some(combo) => match ralfred::hotkey::serve(combo) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("hotkey error: {e}");
                std::process::exit(1);
            }
        },
    }
}

fn cli_expand(args: &[String]) {
    match args.first().map(|s| s.as_str()) {
        Some("list") => ralfred::expand::list(),
        _ => match ralfred::expand::serve() {
            Ok(()) => {}
            Err(e) => {
                eprintln!("expand error: {e}");
                std::process::exit(1);
            }
        },
    }
}
