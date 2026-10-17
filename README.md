# ralfred — Alfred-style launcher in Rust

Clean-room reimplementation of Alfred 5's core feature set, informed by
reverse-engineering Alfred 5.8 (bundle **2348**). No Alfred code, assets, themes,
icons, localisations or licence/activation logic are reproduced; only interface
facts (file names, key names, identifier strings, URL forms, table schemas) are
used, so the two interoperate.

**[`SPEC.md`](SPEC.md) is the normative specification** — behaviour, inputs,
outputs, edge cases and the evidence behind each requirement. This README is the
user-facing summary; where the two disagree, `SPEC.md` wins.

Reverse-engineering method and raw evidence: [`docs/RE_NOTES.md`](docs/RE_NOTES.md)
and [`docs/rea-evidence/`](docs/rea-evidence/). Implementation audit:
[`docs/AUDIT.md`](docs/AUDIT.md).

## Install

**Homebrew** (pulls the DMG from this repo's GitHub Releases):

```
brew tap altruistOS/ralfred https://github.com/altruistOS/Ralfred
brew trust --cask altruistos/ralfred/ralfred   # newer Homebrew only (skips if unneeded)
brew install --cask ralfred
```

**Manual**: grab `Ralfred-<version>.dmg` from Releases, drag **Ralfred.app** into
Applications, launch it. The app lives in the **menu bar (R!)**; press
**Option+Space** (configurable; ⌘, for Settings) to open the panel.
General → *On launch* chooses whether startup opens the panel or stays silent in
the menu bar. First launch after download: right-click → **Open** once (ad-hoc
signature, Gatekeeper).

**Build from source**:
`cd web && npm install && npm run build && cd .. && cargo build --release && bash make_app.sh`.

## Testing

```
cargo test            # 111 unit tests
bash tests/e2e.sh     # 111 end-to-end feature checks, exit 0 = all green
```

The e2e suite runs against a throwaway `RALFRED_HOME` with `RALFRED_NOACT=1`, so
it never touches real user data and never performs irreversible actions.

## Status

| Feature | State |
|---|---|
| Native GUI panel (tao + wry WKWebView) | ✅ borderless rounded panel, always-on-top, hide-on-blur with a stale-blur guard, self-tested pipeline over the `ralfred://` protocol |
| Menu-bar item | ✅ status item with panel / settings / quit |
| Global hotkey | ✅ Carbon `RegisterEventHotKey` + run-loop pump, accessibility-gated, live re-registration, press-only forwarding (a press+release pair must toggle once) |
| Interactive terminal UI | ✅ ratatui; arrows / Enter / Esc, query history |
| App + preference-pane default results | ✅ scope scan, symlink-safe, fuzzy rank, usage recency |
| File/document search | ✅ self-built scoped index (`file_roots`, background build) so typing never blocks; `mdfind` content search is opt-in via `RALFRED_CONTENT_SEARCH=1` |
| File-path search | ✅ `/` and `~/` prefixes with live parent-directory completion |
| Unit-aware calculator | ✅ `+ - * / ^ %`, `sqrt()`, parentheses; data sizes, lengths, masses, **temperatures** (affine) and durations; incompatible conversions are refused |
| Snippets | ✅ `snippets.alfdb`-compatible store, auto-expansion via CGEventTap, and the full dynamic-placeholder grammar (`{clipboard:N.trim}`, `{date +1d:EEEE}`, `{random:1..10}`, `{cursor}`, …) with `ignoredynamicplaceholders` honoured and a recursion bound |
| Workflows | ✅ `.alfredworkflow` zip + unpacked dirs (de-duplicated), keyword routing, graph walk with defunct-edge repair, script filters in **JSON and legacy XML**, full `alfred_*` script environment, `variables` / `rerun` |
| Web-search keywords | ✅ default `g`/`yt`/…, custom keyword→URL map with `{query}` |
| Clipboard history | ✅ `clipboard.alfdb`-compatible, `,` search, `ralfred daemon` poller with frontmost-app attribution, configurable cap |
| URL history | ✅ `urlhistory.alfdb`-compatible |
| Universal Actions | ✅ 23 executable `alfred.action.*` ids with kind-based applicability (file / url / text); unknown ids fail loudly |
| Themes | ✅ imports Alfred `.alfredappearance` JSON as well as native `<uid>.json`, honours alpha, falls back with a report on a bad file |
| Large Type / Quick Look / Dictionary / Contacts / Music / 1Password / Web Bookmarks | ✅ keyword-driven; each external back end is bounded and has a deterministic fixture mode used by tests |
| Install / autostart | ✅ `ralfred install [uninstall|status]` — LaunchAgent + `launchctl`, idempotent |
| Alfred migration | ✅ `ralfred migrate [--dry-run]`: workflows (dirs **and** archives) + snippets/clipboard/url-history via SQLite `ATTACH`, read-only against the source, idempotent |

### Not implemented (deliberate)

- **Powerpack licence / activation** — not reproduced; `ralfred` is free and has
  no key validation.
- **Remote server** — the identifier set was observed, but the wire protocol was
  never examined (no network capture), so it is out of scope rather than guessed.
- **AppleScript dictionary** — Alfred's `.sdef` commands are recorded as
  evidence; `ralfred`'s equivalent surface is its CLI and `ralfred://` scheme.
- **Gallery / Assistant online features** — they depend on Alfred's service.

## Use

```
ralfred                       interactive terminal UI
ralfred safari --open         search and open the top hit
ralfred calc "1gb to mb"      calculator
ralfred '1 gb to mb'          …also works inline from the panel
ralfred snippet sig "text"    add a snippet
ralfred expand                list auto-expanding snippets
ralfred clipboard [--push]    clipboard history (reads stdin with --push)
ralfred actions               list / run Universal Actions
ralfred theme / config        themes and effective configuration
ralfred migrate [--dry-run]   import workflows + databases from Alfred 5
ralfred install [status]      LaunchAgent install / status
ralfred gui [<hotkey>]        native panel with a global hotkey
ralfred --help | --version
```

Config/data: `~/Library/Application Support/ralfred/` (override with
`RALFRED_HOME`). Workflow dir: drop `.alfredworkflow` zips (they are unpacked) or
folders containing `info.plist`.

## Architecture

```
main      CLI verbs + ratatui loop
router    feature factory and result bands: keyword hits > apps > documents
search    app scope scan + fuzzy scoring + knowledge.alfdb recency ranking
index     self-built scoped file index (background), mdfind content fallback
workflow  info.plist graph, script-filter protocol (JSON + XML), alfred_* env
calc      tokenizer + recursive-descent evaluator + affine unit table
db        five SQLite stores, schema-compatible with Alfred *.alfdb
config    JSON config (plist tree kept for interop)
expand    snippet auto-expansion + dynamic-placeholder engine
theme     native theme JSON + .alfredappearance import
gui       tao/wry panel, tray, hotkey, ralfred:// protocol host
```

## Legal

Educational clean-room implementation. Reverse-engineering was conducted on the
shipped app for architecture study only; the binary's licence strings and
activation logic were deliberately not reproduced, and no Alfred asset, theme,
nib, icon or localisation file is redistributed.
