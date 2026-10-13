# ralfred — Alfred-style launcher in Rust

Clean-room reimplementation of Alfred 5's core feature set, informed by
reverse-engineering Alfred 5.8 (see `docs/RE_NOTES.md`). No Alfred code, assets,
themes, or license logic copied; data formats aligned for interop.

## Status (v0.14) → testing: `cargo test` (21 unit) + `bash tests/e2e.sh` (109 feature checks, isolated RALFRED_HOME, exit 0 = all green)

| Feature | State |
|---|---|
| Native GUI window (tao + wry WKWebView) | ✅ borderless rounded panel, centered 22% down, always-on-top, hide-on-blur, self-tested pipeline (route→draw→exec via ralfred:// protocol) |
| Global hotkey summon | ✅ `ralfred gui ALT+CTRL+SPACE` — Carbon RegisterEventHotKey, run-loop pump, accessibility-gated |
| Interactive search box (TUI) | ✅ ratatui, arrow keys / Enter / Esc |
| App + pref-pane default results | ✅ scope scan w/ symlink following, fuzzy rank, usage recency |
| Unit-aware calculator | ✅ + - * / ^ % sqrt(), gb→mb / km→m / hr→s conversions |
| Snippets + keyword expansion | ✅ SQLite `snippets.alfdb` schema-compatible with Alfred |
| Workflow engine | ✅ full graph walk: scriptfilter → connections → openfile/openurl action nodes, action tag on results |
| Web-search keywords | ✅ `g`/`yt`/`wiki`, custom map in config |
| File/document search | ✅ via mdfind (Spotlight) — name + content match, mdfind backend |
| Clipboard history | ✅ `clipboard.alfdb` compatible; `,q` search; `ralfred daemon` polls pbpaste w/ frontmost-app attribution |
| URL history | ✅ `urlhistory.alfdb` compatible |
| File-path search | ✅ `/` and `~/` prefix, live parent-dir completion |
| Universal Actions | ✅ 15 verified `alfred.action.*` mapped: copy/copypath/delete/getinfo/move/open/terminal/reveal/package/large-type/url.open — `ralfred actions` |
| Alfred migration | ✅ `ralfred migrate [--dry-run]`: workflows + snippets/clipboard/history via SQLite ATTACH, read-only, idempotent |
| Native GUI, hotkey, remote server, themes | ⛔ roadmap (see below) |

## Use

```
ralfred                       interactive
ralfred safari --open         search and open
ralfred calc "1gb to mb"      calculator
ralfred snippet sig "text"    add snippet
ralfred clipboard / workflows / config   CLI helpers
ralfred daemon                          clipboard-history poller (pbpaste)
ralfred actions                         list/run universal actions
ralfred migrate [--dry-run]             import workflows + DBs from Alfred 5
```

Config/data: `~/Library/Application Support/ralfred/`.
Workflow dir: drop `.alfredworkflow` zips (they get unpacked) or folders with
`info.plist` — verified against a real Alfred workflow (Snippet Triggers,
11 objects).

## Architecture

```
main      CLI + ratatui loop
router    feature factory: calc > snippet > workflow > web > apps > clipboard/path
search    app scope scan + fuzzy + knowledge.alfdb recency ranking
workflow  info.plist graph, script-filter protocol (env query → JSON items)
calc      tokenizer + recursive-descent evaluator + unit table
db        six SQLite stores, schema-compatible with Alfred *.alfdb
config    JSON config (plist tree layout kept for interop)
```

## Roadmap
1. Native Cocoa window (tao/winit + accesskit) + global hotkey
2. File content search (`mdfind` backend) + icon rendering + Quick Look preview
3. Full workflow node coverage (condition/openfile/output) + connection walking
4. Clipboard native polling, snippet auto-expand via CGEventTap
5. Alfred migration tool: import prefs + workflows + databases from Alfred 5

## Legal
Educational clean-room implementation. Reverse-engineering was conducted on the
shipped app for architecture study only; the binary's license strings and
activation logic were deliberately not reproduced.
- Contacts search (`c <name>` → call/email actions): live AppleScript backend (TCC-guarded, 3s-bounded) + JSON-fixture backend for deterministic tests
- iTunes/Music module (`music <verb>` transport + `music <artist|album>` library search, AppleScript backend bounded by 3s, fixture backend for tests)
- 1Password (`op <query>` → onepassword://search/open_and_fill schemes straight from the binary; fixture backend for tests)
- Icon rendering (icns discovery → 64px sips raster into iconcache keyed by path-hash, served to the GUI rows over ralfred://icon/; qlmanage -t wedges on this machine, sips is the reliable path)
- Install/autostart (`ralfred install [uninstall|status]` — LaunchAgent plist + launchctl bootstrap/bootout; plutil-valid plist, idempotent, sandbox-tested)
- Web Bookmarks (`bm <text>`: Safari AppleScript TCC-aware backend + Chrome JSON stores + fixture)
- Large Type (`lt <text>` fullscreen overlay, ESC dismiss) — **all 13 `AlfredFeature*` domains from the 738-class table are now covered**
