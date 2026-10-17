# ralfred implementation audit

Read-only inventory for spec comparison. Every claim cites `file:line`.

**Baseline / flux.** Audited at HEAD `e9607e5`, working tree dirty. An untracked
`SPEC.md` (898 lines, mode 600) appeared at 00:02. Four files were being edited
*while this audit ran* and kept growing after each read:

| file | at first read | at audit time | at report write |
|---|---|---|---|
| `src/calc.rs` | 241 | 420 | 420 (`c4cf6722409582d1…`) |
| `src/theme.rs` | 239 | 623 | 998 (`a3ba97404c2254d1…`) |
| `src/workflow.rs` | 422 | 1044–1120 | 1138 (`d4dc4df7e3a7a64a…`) |
| `src/expand.rs` | 248 | 248 | 1209 (`73664d7af6b2beea…`) |

Line refs for these four are pinned to the state read at
`2026-10-16T00:09 +08:00` (hashes above are 16-hex prefixes of the *later*
state; `workflow.rs` was `033e14d8228ef88d…` when its body was read).
**Everything else is unmodified from `e9607e5` and stable.** Sections §4, §5 and
§6 rows touching these files are therefore historical and must be re-verified
against the settled tree.

Legend: **[V]** = volatile (file under concurrent edit; content/line numbers have
since changed). Unverifiable items are marked UNKNOWN.

---

## 1. MODULE MAP

Each line: responsibility — public surface — modules it calls.

| file | responsibility | key public surface | calls |
|---|---|---|---|
| `src/main.rs` (556) | CLI verb dispatch + ratatui TUI + one-shot search | `main`, (private) `run_cli_search`, `act`, `run_tui`, `run_loop`, `cli_*` | config, db, router, search, gui, theme, quicklook, icons, install, hotkey, expand, universal, migration, calc, workflow, largetype, onepassword, itunes |
| `src/lib.rs` (37) | module list; re-exports `Config`; `#[cfg(test)]` tests/tests_wf/tests_graph/tests_mig | — | — |
| `src/config.rs` (142) | paths + JSON config | `app_support_dir`, `prefs_dir`, `data_dir`, `workflows_dir`, `Config::{load,save}`, `expand_home`, `default_true` | dirs |
| `src/db.rs` (315) | 5 SQLite stores, Alfred-schema-compatible | `Store::{knowledge,snippets,clipboard,urlhistory,filecache}` + 20 query methods, `Snippet`, `now_ts`, `uuid_v4`, `filecache_upsert` | config, rusqlite |
| `src/router.rs` (308) | feature factory: calc > snippet > workflow > web > contacts/op/lt/bm/music/dict > default > clipboard/urlhist | `Action`, `route`, `docs_search`, `path_candidates`, `exec_action`, `set_clipboard` | calc, features, db, workflow, contacts, onepassword, largetype, webbookmarks, itunes, index, search, config |
| `src/search.rs` (168) | default (app/prefPane) results + fuzzy rank | `Candidate`, `Kind`+`label`, `apps_in`, `fuzzy_score`, `default_results` | config, db, walkdir |
| `src/index.rs` (304) | in-memory scoped file index + rate-limited mdfind fallback | `Entry`, `roots`, `ensure_started`, `docs_candidates`, `mdfind_fallback` | config, search |
| `src/workflow.rs` **[V]** (→1120) | info.plist graph loader + script-filter protocol (JSON/XML) | `wfobj`, `Workflow`{load_dir,run_keyword,run_keyword_report}, `load_workflows`, `unzip_to`, `SfItem`/`SfOutput`, `parse_script_filter*`, `ALFRED_COMPAT_VERSION`, `RERUN_MIN/MAX` | search, plist, zip |
| `src/engine.rs` (265) | "Enter pressed" action path + legacy inline HTML + `items_json` | `act`, `items_json`, `HTML`, `themed` | db, router, search, largetype, onepassword, itunes |
| `src/calc.rs` **[V]** (→420) | unit-aware calculator (bytes/length/time/mass/temperature, offsets) | `calculate`, `is_calc` | (pure) |
| `src/contacts.rs` (181) | Contacts search (`c`/`contacts`) | `parse_applescript_list`, `contacts_search`, `candidates` | search, osascript |
| `src/itunes.rs` (188) | Music transport + library search | `verb_command`, `tell_music`, `candidates`, `run` | search, osascript |
| `src/onepassword.rs` (108) | `op` search → onepassword:// schemes | `candidates`, `run` | search, router |
| `src/webbookmarks.rs` (174) | `bm` Chrome JSON + Safari AppleScript | `all_bookmarks`, `candidates` | search, contacts::parse_applescript_list |
| `src/largetype.rs` (68) | large-type overlay + font scaling | `font_size`, `candidates`, `run` | search |
| `src/quicklook.rs` (148) | qlmanage toggle w/ temp-file materialization | `preview_url`, `available`, `toggle`, `cli` | config |
| `src/expand.rs` (248) | snippet auto-expansion via CGEventTap | `us_char`, `match_expand_rows`, `match_expand`, `Watcher`, `serve`, `list` | db, config, core-graphics |
| `src/migration.rs` (164) | import workflows + DBs from Alfred 5 | `alfred_base`, `MigrationReport`, `migrate` | config, db, workflow, rusqlite |
| `src/install.rs` (175) | LaunchAgent install/uninstall/status | `LABEL`, `use_sandbox`, `plist_xml`, `install`, `uninstall`, `status`, `cli` | config, launchctl |
| `src/settings.rs` (136) | preferences JSON schema + validated PATCH | `schema`, `get`, `patch` | config, hotkey, install |
| `src/features.rs` (89) | feature-domain registry + enable flags | `Feature`, `FEATURES`, `enabled`, `list`, `patch` | config |
| `src/theme.rs` **[V]** (→623) | builtin JSON themes + Alfred `.alfredappearance` import | `Theme`, `ResolvedTheme`, `AlfredAppearance`, `resolve`, `all`, `apply_html`, `import_alfred_appearance`, `cli` | config |
| `src/icons.rs` (97) | icns discovery → 64px sips PNG cache | `icns_for`, `icon_png`, `cli` | config |
| `src/tray.rs` (155) | AppKit status item + Edit menu | `install` | gui (`UserEvent`) |
| `src/hotkey.rs` (231) | global hotkey: Carbon + CGEventTap dual path | `parse_combo`, `LAST_HOTKEY_MS`, `stamp_and_take`, `start_tap_watcher`, `serve`, `list` | config, db, global-hotkey, core-graphics |
| `src/universal.rs` (293) | `alfred.action.*` table | `run_action`, `label`, `applicable`, `known_actions` | router, db |
| `src/gui.rs` (935) | tao window + wry webview, `ralfred://` protocol, selftest | `UserEvent`, `SHOW/HIDE/LT/PREFS/…`, `run` | db, engine, router, index, theme, settings, features, workflow, quicklook, icons, universal, hotkey, tray, webassets |
| `src/webassets.rs` (36) | embeds `web/dist` | `INDEX`, `BUNDLE`, `has_real_assets`, `index_with_theme` | theme |

---

## 2. CLI SURFACE

Dispatch: `main.rs:37-123`. No `--help`/`--version`; unknown first arg = query.

| verb | args | behaviour | cite |
|---|---|---|---|
| *(none)* | — | if `current_exe` contains `.app/Contents/MacOS/` → `gui::run(config.hotkey)`; else ratatui TUI | `main.rs:38-54` |
| `search`/`find` | `[--open] <query…>` | `run_cli_search`; empty query → usage, exit 1 | `main.rs:57-70` |
| `snippet` | `<keyword> <text…>` | `Store::snippets().add_snippet(kw,body,kw)`; <2 args → usage, exit 1 | `main.rs:394-403` |
| `clipboard` | `[q…] [--push]` | `--push` reads stdin `unwrap()` → `clipboard_push(trim,"ralfred")`; else recent(20)/search(20), prints `Hh\tfirstline\t(N bytes)` | `main.rs:405-425` |
| `daemon` | — | 400 ms `pbpaste` poll; insert immediately, async `osascript` frontmost-app attribution, kill after 5 polls | `main.rs:427-476` |
| `workflows` | — | list count + `- name keyword=… objects=…` | `main.rs:477-483` |
| `gui` | `[hotkey]` | `gui::run(Some(hotkey \|\| config.hotkey))`; error → exit 1 | `main.rs:75-98` |
| `theme` | `list\|set <uid>\|show [uid]\|new <uid> [--name/--bg/--text/--subtext/--sel]` | `theme::cli` | `main.rs:99`; `theme.rs:899` |
| `expand` | `[list]` | `list` prints autoexpand keywords; else `serve()` (CGEventTap) | `main.rs:545-556` |
| `hotkey` | `<combo>\|list` | `parse_combo`+`serve` (Carbon+run loop), or `list` from `HK:` knowledge rows | `main.rs:527-543` |
| `actions` | `[<id> [target]]` | no arg → `known_actions()`; else `run_action` | `main.rs:496-513` |
| `ql` | `<path\|text>` | `quicklook::cli` | `main.rs:103` |
| `icon` | `<path>` | `icons::cli` | `main.rs:104` |
| `install` | `[--hotkey K]\|uninstall\|status` | implicit `install` if first arg starts with `--` | `install.rs:144-175` |
| `migrate` | `[--dry-run]` | prints `copied/skipped/db_rows/errors` | `main.rs:515-525` |
| `config` | — | prints hotkey, theme, scope, max_results, web_search, data_dir, workflows | `main.rs:485-494` |
| `calc` | `<expr…>` | `calculate`; None → stderr + exit 1 | `main.rs:108-117` |
| *(catch-all)* | `[--open] <query…>` | joins non-`--open` args; prints `i\ttitle\tsubtitle\tkind[\|action]`; `--open` acts on first; **no results → exit 2** | `main.rs:118-145` |

**Env vars that change behaviour**

`RALFRED_HOME` (`config.rs:9`, `install.rs:29`) · `RALFRED_NOACT=1` suppresses real
actions in `exec_action` only (`router.rs:259`) · `RALFRED_ALFRED_HOME`
(`migration.rs:8`) · `RALFRED_CONTENT_SEARCH` enables mdfind content fallback
(`router.rs:210`) · `RALFRED_INSTALL_NOLAUNCHD` (`install.rs:98,114,126`) ·
`RALFRED_CONTACTS_JSON` (`contacts.rs:23,137`) · `RALFRED_MUSIC_JSON`
(`itunes.rs:71`) · `RALFRED_OP_JSON` (`onepassword.rs:23`) · `RALFRED_BM_JSON`
(`webbookmarks.rs:22`) · `RALFRED_QL_NOEXEC` (`quicklook.rs:67,125,140`) ·
`LT_HOOK_FILE` (`largetype.rs:62`) · `RALFRED_SELFTEST`, `RALFRED_SELFTEST_Q`,
`SELFTEST_TRACE`, `RALFRED_PREFS_AT_START` (`gui.rs:638,656,657,670,814`) ·
`RALFRED_PROTO_DEBUG` (`gui.rs:70`) · `RALFRED_NOGLASS` (`gui.rs:483`) · `HOME`
(`index.rs:49-51`).

---

## 3. DATA LAYER

**On-disk layout** (all under `app_support_dir` = `$RALFRED_HOME` or
`~/Library/Application Support/ralfred`, `config.rs:8-15`):
`Alfred.alfredpreferences/preferences/` (`prefs_dir`, `config.rs:17`) →
`global/ralfred_config.json` (`config.rs:100-102`), `workflows/`
(`config.rs:25`) · `Databases/` (`data_dir`, `config.rs:21`) ·
`Databases/iconcache/` (`icons.rs:12`) · `Databases/quicklook/`
(`quicklook.rs:18`) · `themes/<uid>.json` (`theme.rs:271`) · `gui.sock`
(`gui.rs:545`) · `LaunchAgents/com.kleo.ralfred.plist` when `RALFRED_HOME` set
(`install.rs:23-38`).

**Tables created** (`db.rs`; SQLite opened WAL, `db.rs:12-18`):

- `knowledge.alfdb` (`db.rs:22-33`): `knowledge(item TEXT, keyword TEXT, ts DECIMAL, hidden INTEGER DEFAULT 0)` + indexes `knowledge_item/keyword/ts`; `latching(item, keyword, strong, ts)`; `qhistory(q TEXT, ts DECIMAL)` + `qhistory_ts`.
- `snippets.alfdb` (`db.rs:47-56`): `snippets(uid TEXT PRIMARY KEY, name, keyword, snippet, snippetrtf BLOB, collection, autoexpand BOOLEAN, ignoredynamicplaceholders BOOLEAN)` + `snippets_keyword`, `snippets_autoexpand`; `triggers(keyword, triggeruid, workflowuid)`.
- `clipboard.alfdb` (`db.rs:58-65`): `clipboard(item TEXT, ts DECIMAL, app TEXT, apppath TEXT, dataType INTEGER, dataHash TEXT)` + `clipboard_ts`.
- `urlhistory.alfdb` (`db.rs:67-73`): `history(item TEXT, flatItem TEXT, ts DECIMAL)`.
- `filecache.alfdb` (`db.rs:75-83`): `files(path, fileName, name, nameSearch, nameSplit, nameChars, lastUsed INTEGER, keywords, altnames, filetype INTEGER)` + `files_path`, `files_nameSearch`.

**Reads/writes**: knowledge `record_use`/`last_used`/`last_used_prefix`/
`hide_item`/`record_query`/`recent_queries` (`db.rs:86-225`); snippets
`autoexpand_snippets`/`find_snippets`/`all_snippets`/`add_snippet`
(`db.rs:36-157`); clipboard `push`/`push_now`/`set_app`/`recent`/`search`
(`db.rs:159-194`); urlhistory `url_record`/`url_history` (`db.rs:196-231`);
filecache `filecache_upsert` (`db.rs:276`).

**Spec-relevant gaps**: no `actions.alfdb` / `usedActions` table although
`SPEC.md:757` lists it; knowledge `other`/`recentdocs` tables absent
(`SPEC.md:749-751` vs `db.rs:23-30`). `filecache` table is never populated
(§6 defect 3).

---

## 4. WORKFLOW ENGINE (`src/workflow.rs` **[V]**)

**info.plist parse** (`load_dir`, `workflow.rs:113-232`): `plist::Value::from_file`
(`:118`); `objects[]` → `wfobj{uid, typ, config}` with missing type defaulting to
`"unknown"` (`:121-144`); `connections{from: [{destinationuid, modifiers}]}`
(`:145-168`); dangling edges are dropped with a `warnings` message (`:169-188`);
`name/bundleid/description/version` (`:189-198`); `keyword` = **first object's**
`config.keyword` regardless of node type (`:199-205`); `variables` string map
(`:206-213`).

**Entry selection** (`run_keyword_report`, `:244-326`): scans objects for
`config.keyword == self.keyword`, first match wins (`:248-257`); then dispatches
on entry type only:
- type contains `scriptfilter` → `run_script_filter` (`:259-270`);
- contains `input.keyword` → single pass-through candidate (`:271-281`);
- anything else → pass-through candidate (`:282-292`).

**Connection walk** (`collect_actions`, `:329-348`): BFS, `seen` HashSet, max 3
levels, returns `(uid, modifiers)`. Modifiers are collected but **never used** to
select alternate destinations. Only the **first** action node found is applied,
then `break` (`:296-324`) — later action nodes in the same graph are ignored.

**Handled action types** (`action_of`, `:354-377`) — exact list:
`openfile`, `openurl` (target from `config.paths[0]` with `{query}`/`{var:}`
substitution; falls back to the item arg), and arg-passthrough for `revealfile`,
`terminalcommand`, `systemcommand`, `output.clipboard`, `action.script`.
`{query}`/`{q}` and `{var:NAME}`/`{NAME}` substitution at `:380-388`.

**Script filter** (`run_script_filter`, `:380-425`): stype→shell map
(`:382-392`) = 1 `/bin/bash`, 2 `/bin/zsh`, 3 `/bin/sh`, 4 `php -r`,
5 `ruby -e`, 6 `python -c`, 7 `perl -e`, 8 `python3 -c`, **everything else
(including 0 = AppleScript) → `/bin/zsh -c`**. Env set: `query`, `q`,
`alfred_workflow_version="1.0"`, `alfred_workflow_bundleid=uid`,
`alfred_workflow_name`, `ALFRED_WORKFLOW_VARIABLE_<UPPER>` (`:409-417`).

**Output contract**: `parse_script_filter` (`:506`) → JSON (`:571`, accepts an
`alfredworkflow` root wrapper, requires one of `items`/`variables`/`rerun`) or
legacy XML (`:947`, `output` root). `SfItem` carries
uid/type/title/subtitle/arg/valid/autocomplete/match/icon/quicklookurl/text/mods
(`:457-473`); `SfOutput{items, variables, rerun}` (`:495-501`); `rerun` accepted
only in `[0.1, 5.0]`, silently ignored outside (`:432-435,604`).

**Declared-but-unhandled**:
- Module doc claims `filter`, `conditional`, `output` are run (`:3-5`) — none is
  in `action_of`; they are pass-through/no-op.
- `SPEC.md:564-582` object types with no code path at all: `launchfiles`,
  `applescript`, `script` (bare), `systemwebsearch`, `buffer`, `itunescommand`,
  `browseinterminal`, `browseinalfred`; outputs `notification`, `largetype`,
  `speak`, `playsound`, `writefile`, `callexternaltrigger`, `dispatchkeycombo`;
  utilities `junction`, `conditional`, `filter`, `delay`, `debug`, `dialog`,
  `expression`, `file`, `joinargs`, `json`, `random`, `replace`, `split`,
  `transform`; all triggers except keyword (`hotkey`, `external`,
  `universalaction`, `snippet`, `contact`, `action`, `fallback`, `remote`).
- `SfItem` fields parsed but discarded: `autocomplete`, `match`, `icon`,
  `quicklookurl`, `text`, `mods`, `uid`, `type` — `sf_item_to_candidate`
  (`:532-543`) keeps only title/subtitle/arg/valid.
- `ALFRED_COMPAT_VERSION="5.8"` / `ALFRED_COMPAT_BUILD="2348"` / `PANEL_WIDTH=700`
  (`:43-46`) are declared but not used; invocation still reports `1.0` + folder
  name (`:412-413`).

---

## 5. FEATURE INVENTORY

| module | implemented | backing |
|---|---|---|
| calc | arithmetic `+ - * / ^ %`, parens, unary ±, `sqrt()`, unit conversions (byte/length/time/mass/temperature with offsets) | **real** (pure); `calc.rs:89,119` |
| search/index | app+prefPane scope scan, fuzzy rank, usage recency; self-built name index over `file_roots`, 45 s rebuild, 300 k entry cap; `,`-prefixed clipboard; `,`/`~/` path completion | **real**; `search.rs:53-168`, `index.rs:69-209`, `router.rs:216-243` |
| mdfind | content fallback, 1 try / 5 s | real, **opt-in only** (`RALFRED_CONTENT_SEARCH`, `router.rs:210`, `index.rs:213-222`) |
| snippets | SQLite CRUD, `keyword arg` expansion with `{query}` | **real**; `router.rs:47-65`, `db.rs:133-157` |
| expand **[V]** | CGEventTap autoexpand, backspace+paste | **real**, Accessibility-gated; original read `expand.rs:211-236` (now 1209 lines) |
| clipboard | `,q` search, `--push`, daemon pbpaste poll + frontmost app | **real**; `router.rs:168-180`, `main.rs:427-476` |
| universal | file/URL/text action sets via open/osascript/FS | **real**, but 2 ids are no-ops (§6.9/6.10); `universal.rs:9-128` |
| contacts | `c`/`contacts` search → tel:/mailto: actions | **fixture** `RALFRED_CONTACTS_JSON` **or** live AppleScript 3 s-bounded (`contacts.rs:40-73`); no AddressBook DB read |
| itunes | `music <verb>` transport; `music <text>` library | transport **real** AppleScript; library search **fixture-only** (`itunes.rs:70-83`), no live library read |
| onepassword | `op` search → `onepassword://search/view/open_and_fill` | item list **fixture-only** (`onepassword.rs:22-30`); no live 1Password backend |
| webbookmarks | `bm` title/url search | **fixture** or live Chrome JSON + Safari AppleScript (`webbookmarks.rs:32-115`) |
| largetype | `lt <text>`, fullscreen overlay, auto-fit 180/110/64/40/24 px | **real** GUI overlay (`gui.rs:720-736`); CLI prints (`largetype.rs:59-67`) |
| quicklook | `ql`, toggle qlmanage, temp file for text | **real** (`quicklook.rs:66-116`); NOEXEC test mode |
| migration | workflows copy + SQLite ATTACH row import | **real** but broken workflow discovery (§6.1/6.2) |
| install | LaunchAgent plist + `launchctl bootstrap/bootout` | **real**, NOLAUNCHD hook (`install.rs:86-142`) |
| settings | 5-section schema, validated PATCH, login-item toggle | **real** (`settings.rs:9-136`) |
| theme **[V]** | 3 builtins, custom JSON, glass alpha, `.alfredappearance` import | **real** **[V]** (`theme.rs`) |
| icons | icns → 64 px PNG via `sips`, path-hash cache | **real** (`icons.rs:30-80`) |
| tray | menu-bar item `R!` + Panel/Preferences/Quit + Edit menu | **real** (`tray.rs:13-151`) |
| hotkey | Carbon RegisterEventHotKey + CGEventTap watcher, live re-register | **real** (`hotkey.rs:124-208`, `gui.rs:566-629`) |
| gui | tao window + wry WKWebView, `ralfred://` protocol, 25 routes, React bundle | **real** (`gui.rs:473-916`) |
| features | 14 feature domains, enable flags honored by router | **real** (`features.rs:16-31`) |
| webassets | embeds `web/dist/{index.html,bundle.js}` (197 968 B real bundle) | **real** (`webassets.rs:7-13`) |

**`TODO`/`FIXME`/`unimplemented!`/`todo!()`/`XXX`/`HACK`: none found** in `src/`
or `web/src` (grep, exit 1). Absence of markers is not absence of gaps — see §4/§6.

---

## 6. INCONSISTENCIES AND BUGS

**HIGH**

1. `migration.rs:98` drops any path containing
   `Alfred.alfredpreferences/preferences` — i.e. exactly the Alfred 5 layout the
   module claims to import (`migration.rs:32-33`). On a real install
   `find_workflows` returns nothing. `tests_mig.rs:12` only passes because it
   uses `Alfred.alfredpreferences/workflows/…` (no `preferences/` segment).
2. `migration.rs:106-116`: the `.alfredworkflow` zip-unpack branch is nested
   inside `if p.is_dir()` (`:97`), but a `.alfredworkflow` is a file → zipped
   workflows are never discovered or unpacked.
3. `db.rs:276-283` `filecache_upsert` uses `ON CONFLICT(path)` but `files` has no
   PRIMARY KEY/UNIQUE on `path` (only an index, `db.rs:79`) → SQLite errors at
   runtime; the error is swallowed by `.ok()`. The function also has **0
   call-sites** (dead code), so the file cache is never written.
4. `main.rs:147-254` `act()` is a full duplicate of `engine.rs:8-127` `act()` and
   the two disagree: `engine.rs:81-101` opens a Snippet whose text is
   `http(s)://`/`www.` as a URL; `main.rs:224-228` always pastes the literal
   text. CLI/TUI and GUI behave differently for the same result.
5. `gui.rs:918-935` `url_decode`: `&s[i+1..i+3]` slices bytes at a fixed offset;
   a query containing `%` followed by a multibyte char (e.g. `%é`) panics on a
   non-char boundary. Reachable from the `ralfred://api/...` custom-protocol URL.
   Malformed escapes (e.g. `ab%4`) are also silently passed through.
6. `main.rs:409` `io::stdin().read_to_string(&mut s).unwrap()` — `ralfred
   clipboard --push` panics on non-UTF-8 stdin.
7. `main.rs:268` `setup_terminal().expect("init terminal")` — bare `ralfred`
   (no args, not in a bundle) panics when stdout is not a TTY because
   `enable_raw_mode` fails.

**MEDIUM**

8. `universal.rs:126` `_ => Ok(())` — unknown action ids report success while
   doing nothing. `alfred.action.openwith` is advertised in `known_actions()`
   (`:228`) and labelled (`:147`) but has no arm.
9. `universal.rs:93-98` `alfred.action.text.saveassnippet` stores an empty
   keyword; router snippet expansion requires a non-empty `kw`
   (`router.rs:48-52`), so the snippet can never be expanded.
10. `search.rs:117` compares `t.as_bytes()[ti] as char != qc`, mixing bytes and
    chars; `search.rs:129` compares byte `hits` against `q.chars().count()`.
    Fuzzy subsequence matching is wrong for any non-ASCII query/target.
11. `db.rs:117-120` `hide_item` has 0 call-sites and `last_used`/
    `last_used_prefix` (`db.rs:99,110`) never filter `hidden` — the column is dead.
12. `db.rs:28` `latching` table is created but never read or written (0
    call-sites for `latching`). `docs/RE_NOTES.md:28-31` claims it drives ranking.
13. `db.rs:163,172` clipboard retention is hardcoded `LIMIT 500`; the
    `clipboard_max_items` setting (`config.rs:38`, `settings.rs:30`) is never
    read, so "Clipboard history size" has no effect.
14. `settings.rs:33` exposes `default_scope` as "Folders to search", but document
    search reads `Config::file_roots` (`index.rs:38-45`), which is **absent from
    the settings schema** → the GUI label controls app scope and index roots
    cannot be edited from Settings.
15. `router.rs:196`, `search.rs:165`, `itunes.rs:135` use
    `partial_cmp(...).unwrap()`. Any NaN candidate score panics the process.
16. **[V]** `expand.rs:151-153`: `c => if c == 49` is unreachable — `SPACE` (49)
    already matched at `:139`. Residue of an earlier ordering (pre-rewrite refs).
17. **[V]** `expand.rs:122` loads `auto_expand_snippets` once into `Watcher.cfg`,
    and `serve()` (`:211`) never calls `reload()` — new/edited autoexpand snippets
    require an app restart (pre-rewrite refs).
18. `index.rs:114-131` `build_guarded` spawns a walker and abandons it on
    timeout; each refresh cycle on a wedged mount leaks one OS thread (documented
    at `:111-113`).
19. **[V]** `workflow.rs:412-413` still sends `alfred_workflow_version="1.0"` and
    `bundleid = folder name`, ignoring `ALFRED_COMPAT_VERSION` (`:43`) and the new
    `bundleid` field (`:25`) — third-party version gates see 1.0.
20. **[V]** `workflow.rs:296-324` attaches only the first action node's tag and
    `break`s; multiple action nodes / modifier-specific edges are dropped, and
    `mods` from `collect_actions` is ignored.
21. `main.rs:75-98`: the `gui` hotkey `or_else` closure (`:78-86`) always returns
    `None` (both the inner `return None` and the trailing `None`), so the whole
    branch is dead code; the effective value is `args[1]` or `config.hotkey`.
22. `contacts.rs:56-68`: after `try_wait()` returns `Some` (child reaped),
    `wait_with_output()` is called again; stdout may come back empty.

**LOW**

23. **[V]** `calc.rs:127-143` `calc_tokens` guards with `"a-zA-Z_".contains(c)`,
    which only matches the literal chars `a - z A Z _`; e.g. `is_calc("1+bcd")`
    returns true although `calculate` returns `None` (harmless fall-through).
24. `main.rs:118-122` treats `ralfred --help` / `--version` as a search query.
25. `webbookmarks.rs:110-114` reuses the 3-list contacts parser; a 2-group Safari
    reply silently shifts the URL into the phone slot (acknowledged in comment).
26. `hotkey.rs:157-159` prints up to 3 debug lines per tap installation
    (`[hk tap] event #…`) to stderr unconditionally.
27. `gui.rs:344` `wb[..n]` and `gui.rs:381` `c[..6]` slice theme colour strings at
    fixed byte offsets — a custom theme JSON with multibyte values panics.
28. `gui.rs:195-206` builds `/history` JSON escaping only `"` and `\`; control
    characters in a typed query can produce invalid JSON.
29. `index.rs:224-227` interpolates the raw query into the `mdfind` query string
    without escaping.
30. `workflow.rs:3-5` module comment claims `filter`/`conditional`/`output` run —
    false (see §4). `tests/e2e.sh:352` has a duplicate group label
    `== 8.6 hotkey ==` after `== 11`.

---

## 7. TEST INVENTORY

`#[test]` count per file as measured during the audit (total **54**; the three
volatile files were still gaining tests):

- `src/tests.rs` (18): `arithmetic`, `conversions`, `detector`, `invalid`,
  `fuzz`, `expand_match`, `expand_keycodes`, `expand_watcher_buffer`,
  `contacts_fixture_and_parse`, `itunes_verbs_and_fixture`,
  `quicklook_url_and_toggle`, `icons_and_onepassword`,
  `install_lifecycle_sandboxed`, `webbookmarks_fixture`,
  `largetype_candidates_and_scale`, `hotkey_combo_robust_parsing`,
  `settings_get_patch_roundtrip`, `features_enable_disable_gating`.
- `src/tests_wf.rs` (4): `script_filter_json`, `script_filter_plain_lines`,
  `script_filter_empty`, `workflows_dedupe_after_unpack`.
- `src/tests_graph.rs` (2): `graph_without_action_node`,
  `graph_with_openfile_action`.
- `src/tests_mig.rs` (1): `migrate_from_fake_home`.
- `src/calc.rs` **[V]** (13): `arithmetic_regression`,
  `linear_conversions_regression`, `temperature_celsius_to_fahrenheit`,
  `temperature_fahrenheit_to_celsius`, `temperature_kelvin_all_directions`,
  `temperature_aliases`, `temperature_is_not_a_scale_ratio`,
  `incompatible_dimensions_yield_nothing`, `currency_is_not_claimed`,
  `malformed_conversion_inputs`, `detector_regression`, `base_unit_reported_once`,
  `unit_lookup_is_case_and_space_insensitive`.
- `src/theme.rs` **[V]** (9), `src/db.rs` (1 `query_history_dedupes_and_orders`),
  `src/features.rs` (2), `src/index.rs` (1), `src/universal.rs` (2),
  `src/webassets.rs` (1).
- `workflow.rs` had **0** inline tests at read time; graph/JSON coverage lives in
  `tests_wf.rs`/`tests_graph.rs`.

Env-touching tests serialize on `INSTALL_TEST_LOCK` (`tests.rs:219`) plus
`env_home_lock` (`db.rs:292`); some pin `RALFRED_HOME` directly
(`db.rs:302`, `index.rs:272`, `tests.rs:319,349`, `tests_mig.rs:27-31`).

**No test coverage**: `gui.rs` protocol routes and selftest, `engine.rs`
`act`/`items_json`/HTML, `router.rs::route` (only indirectly via
`tests.rs:362`), `search.rs::default_results`/`apps_in`, `migration.rs`
`find_workflows` layout matching, `settings.rs` schema rendering, `tray.rs`,
`quicklook.rs::available`, `icon.rs::cli`, the daemon poll loop, and
`hotkey.rs::serve`/`start_tap_watcher`.

**`tests/e2e.sh`** (359 lines): isolates state with
`H=$(mktemp -d /tmp/ralfred_e2e.XXXXXX)`, `export RALFRED_HOME="$H"`,
`RALFRED_ALFRED_HOME="$H/fakealfred"`, `RALFRED_NOACT=1` (`:6-12`); helpers
`chk` (substring present) and `chk_absent` (`:15-28`); a second
`H2=$(mktemp -d)` for settings/theme (`:152`, `trap` EXIT at `:152`). Groups:
1 calculator · 2 snippets · 2a2 dictionary · 2a2b contacts fixture · 2a2c music
fixture · 2a2d quicklook · 2a2e 1Password+icons · 2a2f install sandbox ·
2a2g bookmarks fixture · 2a2h large type · 2a2i settings · 2a3 themes ·
2b autoexpand · 3 workflows (incl. zip unpack, variables, clipboard output,
keyword input) · 4 search · 5 clipboard · 6 universal actions · 7 migration ·
8 config · 9 docs (soft) · 10 daemon (soft) · 11 no-act safety · GUI selftest +
theme e2e (`:316-330`) · hotkey (`:340-350`). ~**101** `chk`/`chk_absent`
invocations plus ~8 inline `if` checks. Fixtures are pointed at via
`RALFRED_CONTACTS_JSON`, `RALFRED_MUSIC_JSON`, `RALFRED_OP_JSON`,
`RALFRED_BM_JSON`; live backends are disabled by exporting those vars empty
after the fixture group (`:79,96,116,143`).

---

## 8. DOC DRIFT

`README.md`

- `:26` "**32 unit**" — 45 at first read, **54** by end of audit; "**111 feature
  checks**" — ~101 `chk` + ~8 inline checks.
- `:44` "Native GUI, hotkey, remote server, themes | ⛔ roadmap" contradicts
  `:30-31` (GUI ✅, hotkey ✅) and the code (`gui.rs`, `hotkey.rs`, `theme.rs`).
- `:38` "File/document search ✅ via mdfind (Spotlight) … mdfind backend" — code
  is a self-built index (`index.rs`); mdfind only when `RALFRED_CONTENT_SEARCH`
  is set (`router.rs:210`).
- `:42` "Universal Actions ✅ **15** verified" — `known_actions()` has **24** ids
  (`universal.rs:216-243`).
- `:72` "db **six** SQLite stores" — `db.rs` has **5** `open()` calls
  (knowledge/snippets/clipboard/urlhistory/filecache).
- `:76-81` Roadmap items 1-5 are all already implemented (GUI+hotkey; content
  search/icons/Quick Look; workflow nodes; clipboard polling + CGEventTap
  autoexpand; migration).
- `:26` / `make_app.sh:44-45` say **v0.14**, but `Cargo.toml:3` and
  `Casks/ralfred.rb:2` say **0.1.0**.
- `:93` "all **13** `AlfredFeature*` domains" — `FEATURES` has **14** entries
  (`features.rs:16-31`).
- `:87-93` bullet list is appended after the `## Legal` heading (`:83`).

`docs/RE_NOTES.md`

- `:21` lists a `filecache/…/knowledge/**actions**` DB set — no `actions` store
  exists in `db.rs` (spec `SPEC.md:757` also expects `actions.alfdb`).
- `:28-31` says `latching` and `hidden` drive ranking — neither is used
  (`db.rs:28,117`).
- `:25` script-filter field list implies `uid`/`autocomplete`/`icon`/`mods` are
  consumed — `sf_item_to_candidate` drops them (`workflow.rs:532-543`).

In-code

- `workflow.rs:3-5` claims `filter`/`conditional`/`output` are run; §4 shows they
  are not.
- `tests/e2e.sh:352` duplicate group label, and group numbering jumps
  `== 8.6` after `== 11` (`:332`).

---

## UNKNOWN / NOT DETERMINED

- Final post-edit content and line numbers of `src/workflow.rs`, `src/theme.rs`,
  `src/calc.rs` (files were mutating throughout; see flux note).
- Whether `SPEC.md` (untracked, mode 600) is the authoritative comparison
  baseline for the reviewer — its `§11 Conformance checklist` was not audited.
- Runtime behaviour of `gui.rs` protocol routes, CGEventTap paths, and the
  selftest under a real Accessibility grant (no execution performed).
- Whether any `.alfredworkflow` sample beyond the two synthetic fixtures parses
  correctly (only fixture plists were available).
- `docs/rea-evidence/raw/` contents (not read).
