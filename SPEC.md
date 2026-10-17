# SPEC — ralfred: a clean-room Alfred 5-compatible launcher

Status: normative specification, revision 1.
Target of study: `/Applications/Alfred 5.app` — `CFBundleShortVersionString` **5.8**,
`CFBundleVersion` **2348**, bundle id `com.runningwithcrayons.Alfred`.
Deliverable: the `ralfred` project in this repository.

This document is the single source of truth for what `ralfred` must do. It was
written from reverse-engineering evidence about Alfred 5 (method and raw evidence
in `docs/rea-evidence/`), then used to build and verify the implementation.

**Clean-room notice.** No Alfred source, object code, asset, nib, theme file,
icon, localisation string, or licence/activation logic is reproduced here or in
the implementation. What is recorded is *interface*: file names, directory
layout, key names, environment-variable names, identifier strings, URL forms,
and table schemas. Those are facts required for interoperability, and are the
only things taken from the target. Every design decision about how to *implement*
that behaviour is made independently and is marked as such.

Evidence labels used throughout:

| Label | Meaning |
|---|---|
| **[O]** | Observed — returned by an REA tool or read directly from a shipped/on-disk file. |
| **[D]** | Derived — computed by this investigation from observed data. |
| **[I]** | Inferred — plausible mechanism not directly observed. |
| **[C]** | Choice — a `ralfred` design decision, not a claim about Alfred. |

---

## 1. How the evidence was obtained

Provider state at investigation time (`binary_session`): Hopper's GUI bridge was
unavailable (macOS denied the driving process permission to send it Apple
Events); Ghidra 12.1.4 was installed but its native decompiler binary for
`darwin/arm64` was missing. Deep decompilation was therefore unavailable, and
**no control-flow-level claims are made in this document.** The investigation
used the providers that were available:

| Provider / method | Used for |
|---|---|
| `open_binary`, `inspect_macho` | Mach-O identity, slices, UUIDs, load commands, exported/imported symbols |
| `inspect_native_dispatch_metadata` | Objective-C class / method / protocol inventory [O] |
| `decode_interface_builder` | Compiled nib/xib keyed-archive trees [O] |
| `inspect_artifact`, `inspect_asset_catalog` | Bundle inventory, asset catalogs |
| `inspect_sqlite_database`, `sqlite3` on live stores | Database schemas [O] |
| `plutil` / direct reads of shipped plists, JSON, and zipped workflows | Preference trees, theme format, workflow format [O] |
| `strings -a` on the shipped framework binary | Identifier and literal inventories [D] |
| Direct observation of the running application's preference panes | Default Results / File Search / Appearance configuration and panel anatomy [O] |

Bounded negative results are stated as such. The largest coverage caveat is that
the ObjC metadata reader decoded **279** non-metaclass classes for the framework
while roughly **780** distinct `Alfred*` names exist as strings; the extra names
are nib class references, categories, or records the reader did not decode.
Class-name coverage is broad; method-map coverage is bounded to decoded records.

---

## 2. Observed anatomy of the target

### 2.1 Bundle layout [O]

```
Alfred 5.app/Contents/
  Info.plist                      # registers URL scheme "alfred", NSAppleScriptEnabled
  MacOS/Alfred                    # 202 KB arm64 slice — thin bootstrap/launcher
  Frameworks/
    Alfred Framework.framework/Versions/A/Alfred Framework   # 11.4 MB dylib — the engine
    libConfigurer64.dylib
  Preferences/Alfred Preferences.app          # separate preferences GUI helper (~0.5 MB)
  Resources/
    Alfred.sdef                   # AppleScript dictionary (public scripting API)
    Assets.car, *.icns, *.tiff, *.lproj
  Frameworks/…/Resources/         # 131 compiled nibs, 9 .alfredappearance themes,
                                  # bundled sample .alfredworkflow files,
                                  # "Alfred Defaults.plist", "Alfred Preferences Defaults.plist"
```

Signature facts [O]: identifier `com.runningwithcrayons.Alfred`, hardened runtime
**false**, team id `null`.

Document/UTI registrations [O]: `.alfredpreferences` (LSTypeIsPackage),
`.alfredworkflow` + `.alfred3workflow`/`.alfred4workflow`/`.alfred5workflow`
(zip archive), `.alfredappearance` (conforms `public.json`), `.alfredsnippets`.

### 2.2 Feature surface [O]

Non-metaclass Objective-C classes with decoded method records: 21 (launcher) +
279 (framework) + 55 (preferences helper). Sixteen `AlfredFeature*` domains were
decoded:

`1Password`, `Calculator`, `Clipboard`, `Contacts`, `DefaultSearch`,
`Dictionary`, `FileSearch`, `LargeType`, `Previews`, `Snippets`, `System`,
`Terminal`, `UniversalActions`, `WebBookmarks`, `WebSearch`, `iTunes`.

Supporting machinery observed as classes: `AlfredHotkeyManager`/`AlfredHotkeyCombo`
and `AlfredKeystrokeHelper`; `AlfredKnowledgeManager`; `AlfredThemeManager`,
`AlfredThemeImporter`, `AlfredThemeExporter`; `AlfredWorkflowController`,
`AlfredWorkflowCanvasController`; `AlfredDefaultResultsView`, `AlfredDefaultView`,
`AlfredWindow`; `AlfredDistributedNotificationManager`; `AlfredUserNotificationManager`;
`AlfredRemoteAction*`; `AlfredUniversalAction*`; `AlfredAppleScriptCommand*`.

`ralfred` implements the behaviour of these domains as specified in §6. Features
whose behaviour could not be established are listed as non-goals in §10.

---

## 3. Core behaviour

### 3.1 The launch surface [O + I]

Alfred owns a borderless, rounded, always-on-top panel (`AlfredWindow`, an
`NSPanel`) that is summoned, populated with a query, and dismissed. Method names
on the decoded classes give the following lifecycle [O], [I] for ordering:

1. A global hotkey, an `alfred://` URL, an AppleScript command, a workflow
   trigger, or a click re-veals the panel.
2. The panel is prepared (theme, blur, focus, roundness, geometry), then shown
   with an optional input string and optional argument/variables.
3. Each keystroke runs the query pipeline (§3.2) and re-renders the result list.
4. Enter actions the highlighted result; Escape hides the panel.
5. The panel hides on resign-key (blur), subject to a guard so that a stale blur
   arriving while the panel is being re-ordered cannot hide a fresh summon.

`ralfred` uses a borderless always-on-top window, hides on genuine focus loss,
and ignores blur within a short window after a summon. **[C]**

**Panel anatomy, observed in the running application [O]** (Appearance pane
live preview). The panel is a single column: the query row (typed text plus a
mode indicator — the hat icon — at its right), then the result list. Each
result row carries an icon, a title, a subtitle, and — for the first nine
results — a right-aligned ⌘N hint, so ⌘1…⌘9 directly action the nth row. The
subtitle is the item's locator: for an application it is the bundle path
(`/Applications/Mission Control.app`). The highlighted row is filled with the
theme's selected colour and shows a ↵ affordance at its right edge, i.e. Enter
is the primary action and is advertised on the focused row. The query and
selection state are therefore readable from the panel alone.

Required behaviour (implemented in `ralfred`): the CLI prints
`index, title, subtitle, kind` per row, which is the same four fields the
panel renders minus the icon and shortcut hint; ⌘1…⌘9 and Enter are bound per
§4.1.

### 3.2 Query pipeline [O for the primitives]

Per keystroke the panel processes the search field and pushes a query update to
the results view; results are then re-rendered. The router resolves a query in
this order:

1. **Unit-aware calculator** if the query is an arithmetical expression (§6.8).
2. **Keyword features** — a leading keyword token followed by an argument
   dispatches to that feature (§5.1).
3. **Workflow script filters and other workflow inputs** (§7).
4. **Default results** — applications, preference panes, and configured scope
   entries, fuzzy-matched and ranked by usage knowledge (§6.1).
5. **Fallbacks** — if nothing matched, configurable fallback results are offered
   (e.g. a web search for the typed text).

A keyword already claimed by another feature must not run: Alfred reports
"the keyword you have typed cannot run due to a keyword conflict" [O].

### 3.3 Result model [O]

A result is a row carrying at minimum a title, optional subtitle, an icon
reference, an action payload (arg), and identity metadata used for ranking and
for the Universal Actions menu. Script-filter results additionally carry
`uid`, `valid`, `autocomplete`, `match`, `quicklookurl`, `text{copy,largetype}`,
`mods{...}` and `type` (§7.3).

### 3.4 Ranking [O]

Alfred persists usage in `Databases/knowledge.alfdb` with these observed tables
and columns:

| table | columns |
|---|---|
| `knowledge` | `item, keyword, ts, hidden` |
| `latching` | `item, keyword, strong, ts` |
| `other` | `type, uid, data, context, ts` |
| `recentdocs` | `path, bundle, ts` |

Observed statements include inserting a knowledge row with and without the
`hidden` flag, inserting a latching row with `strong = 0`, clearing latching, and
counting non-hidden knowledge entries in a time window [O]. From this the
required behaviour is:

- Every actioned result records a usage event `(item, keyword, ts)`.
- Items flagged hidden are excluded from ranking but retained.
- Latching (`strong`) makes a result sticky for a keyword.
- Recency dominates: a more recently used item outranks an older one for the
  same query, and ties break deterministically.
- Recent documents are tracked per bundle: `recentdocs(path, bundle, ts)`,
  queried as the newest timestamp per path for a bundle, capped at 20 rows [O].

### 3.5 Panel geometry and theme [O]

A shipped theme file records `alfredtheme.window.width` plus paddings, roundness
and blur. Observed widths across the shipped set are **559** (the theme named
"Alfred"), and 573 / 683 / 700 in newer files. `ralfred` derives its panel width
from the active theme and positions the panel horizontally centred, vertically in
the upper part of the screen. **[C]** (Exact Alfred placement is not established.)

---

## 4. Inputs

### 4.1 Keyboard [O]

The search field handles these command selectors: `insertNewline`, `insertTab`,
`cancelOperation`, `moveUp`, `moveDown`, `moveLeft`, `moveRight`,
`deleteBackward`, `deleteWordForward`, `selectAll`, `scrollPageUp`,
`scrollPageDown`. Observed preference keys indicate the modifier-bound gestures:

| Gesture | Preference key | Required behaviour |
|---|---|---|
| Show actions panel on selection | `features.filesearch.actions.showWithTab` | Tab opens the actions menu for the highlighted result |
| Right-arrow navigation | `features.filesearch.actions.showWithRightArrow` | Right arrow acts on the selection |
| Quick Look preview | `features.filesearch.quicklook.showWithShift` | Shift previews the highlighted file |
| Navigation with arrows | `features.filesearch.navigation.*` | Arrows drive folder navigation (§6.2) |

`ralfred` additionally specifies, as a design choice: Up/Down move the highlight;
Enter actions; Escape hides; Ctrl+Up/Down walk the typed-query history; Up with
an empty query recalls the previous query; Cmd+, opens preferences; Cmd+L shows
the selection as Large Type. **[C]**

### 4.2 Global hotkey [O + I]

`AlfredHotkeyCombo` exposes `mod`, `key`, `hotKeyID`, `hotKeyRef`; the manager
exposes `addHotkey:`, `deregisterCombo`, `findHotkeyComboWithMod:key:`,
`focusedAppChanged:`, `hotkeyRelevantToFocusedApp`, `addTapListener:`, `checkTap`
[O]. **[I]** Alfred registers per-combo system hotkeys and supplements them with
an event tap for app-scoped combos.

Required behaviour for `ralfred`: a configurable global hotkey summons or
toggles the panel; the combination is parsed from a textual form; an invalid
combination is rejected without crashing; re-configuring re-registers live; a
single physical press must produce exactly one toggle — a press that emits both
a key-down and a key-up event must be forwarded once, otherwise the panel
flashes and hides. The default combination is Option+Space. **[C]**

### 4.3 Command-line [C]

The deliverable is a single binary with these verbs. Every verb must be
non-interactive and testable in an isolated home.

```
ralfred [<query>] [--open]        interactive TUI, or one-shot query
ralfred search <text>             default-results search
ralfred calc <expr>               calculator
ralfred snippet <kw> [<text>]     list/add snippets
ralfred clipboard                 clipboard history operations
ralfred workflows [<verb>]        list / run / inspect workflows
ralfred actions [<verb>]          list / run Universal Actions
ralfred config <get|set|path>     configuration
ralfred migrate [--dry-run]       import from Alfred 5
ralfred daemon                    background clipboard poller
ralfred install [uninstall|status]
ralfred gui [<hotkey>]            native panel
```

Environment: `RALFRED_HOME` overrides the data root; `RALFRED_NOACT=1` suppresses
irreversible real-world actions (used by tests). Data root default:
`~/Library/Application Support/ralfred/`.

### 4.4 URL scheme [O]

The bundle registers the `alfred` scheme. Observed handlers include:

```
alfred://runtrigger/<workflow-uid>/<object-uid>?argument=<value>
alfred://customsearch/<uid>/<name>/<emoji>/<url>/<keyword>
alfred://gallery/
```

The launcher's URL entry point is `application:openURLs:` → `handleURLCommand:`,
and a `findAndRunTriggerFromURLPath:` step resolves the trigger [O]. `ralfred`
must accept the same trigger form and run the named workflow object.

### 4.5 AppleScript [O]

`Contents/Resources/Alfred.sdef` declares exactly these commands, which define
the public scripting contract:

| Command | Direct parameter | Named parameters |
|---|---|---|
| `search` | text (optional) | — |
| `action` | list of text | `as type` (file \| url \| text) |
| `browse` | text | — |
| `run trigger` | trigger identifier | `in workflow`, `with argument` |
| `reload workflow` | uid or bundle id | — |
| `reveal workflow` | uid or bundle id | `configuration`, `details` |
| `set configuration` | variable name | `to value`, `in workflow`, `exportable` |
| `remove configuration` | variable name | `in workflow` |
| `set theme` | theme name | — |

`ralfred` is not required to expose an AppleScript dictionary (it is not an
`NSScriptCommand` host). Its equivalent surface is the CLI and the `ralfred://`
scheme. **[C]** This is a recorded divergence, not a claim of parity.

### 4.6 Files ingested [O]

`.alfredworkflow` (and the v3/v4/v5 extensions) — a zip containing `info.plist`.
`.alfredsnippets`. `.alfredappearance` — JSON theme. Workflow folders containing
`info.plist` are equivalent to the archive form and must be de-duplicated against
an unpacked copy of the same archive.

---

## 5. Feature specification

### 5.1 Default results [O]

Scope comes from the on-disk preference tree. Observed example:
`preferences/local/<hash>/features/defaultresults/prefs.plist` containing a
`scope` array of directories. Also observed as preference keys:
`features.defaultresults.{showAll, fallbacks, applicationFuzzyType,
applicationUseKeywords}` [O].

Required behaviour: scan configured scope roots for `.app` bundles and preference
panes; support `~/` and absolute paths; follow symlinks without looping; rank by
fuzzy match over the name and by usage recency (§3.4); expose results directly
from the panel. The result set must be stable for identical inputs and state.

**Default Results pane, observed in the running application [O]** (Alfred →
Features → Default Results). The default result *types* are grouped into named
buckets, not a flat list:

| Bucket | Members (observed on this install) |
|---|---|
| Applications | `Options…` (application search options) |
| Essentials | Contacts ✓, System Settings ✓ |
| Extras | Folders, Documents, Text Files, Images, Archives, AppleScripts — all unchecked, plus `Advanced…` |
| Unintelligent | `Search all file types` — unchecked, with the warning "Selecting this will slow down and muddle the default search results considerably" |
| Fallbacks | `Only show fallbacks when there are no results` (the observed setting), plus `Setup fallback results` |

The pane's own note states Alfred "works most efficiently if you only have
'Essential' items ticked and use the 'open' keyword to find files", and that
pressing `[spacebar]` immediately after activating Alfred enters file search
mode — i.e. the default result set is deliberately narrow and file search is
the escape hatch. The fallback note states "By default, Alfred shows Google,
Amazon and Wikipedia".

Required behaviour (implemented in `ralfred`): the default-result set is
limited to applications and preference panes under the configured scope; the
fallback band runs only when no other band produced a result, and offers web
searches for the typed text (§3.2 step 5). The Essentials/Extras typing model
is the reason `ralfred` does not index documents in the default band.

Two boundaries govern the fallback band, recorded as deliberate narrowings of
the pane's "when there are no results" wording rather than as observations: it
fires only for a **single-token** query, so a mistyped multi-word feature
keyword (`bmzz rust`) is not silently converted into a web search; and it never
fires for a query that named a **file-search mode** (`'`, `open`, `find`, `in`,
`tags`), because such a query explicitly asked for files and must not have its
mode prefix echoed into a search URL (SPEC §8.21).

### 5.2 File search [O]

Preference keys observed: `features.filesearch.*` including actions, preview,
buffer, navigation and result-limit keys. Two mechanisms exist in Alfred:
a Spotlight-backed search and an internal file cache (`Databases/filecache.alfdb`
with columns `path, fileName, name, nameSearch, nameSplit, nameChars, lastUsed,
keywords, altnames, filetype`) [O].

Required behaviour: path-prefixed queries (`/`, `~/`) enter file mode and offer
live parent-directory completion; a Space in an empty query box enters file mode;
name and content matching are both supported; if the system index is unavailable
the tool must degrade gracefully rather than fail. `ralfred` maintains its own
scoped index built in the background so that per-keystroke search never blocks
the UI thread. **[C]** for the index design; the `filecache` schema is [O] and is
used for interoperability.

**File Search pane, observed in the running application [O]** (Alfred →
Features → File Search). The pane has four tabs — *Search, Navigation, Buffer,
Advanced* — and the Search tab declares:

| Control | Observed value |
|---|---|
| Quick Search | `Enable Quick File Search mode` — prefixed with `'` *or* the spacebar |
| Opening Files | keyword `open` — the keyword followed by the search term opens the file |
| Revealing Files | keyword `find` — reveals the file in Finder |
| Inside Files | keyword `in` — searches the contents of a file |
| File Tags | keyword `tags` — matches files tagged with all matching terms |
| Don't Show | Emails, Bookmarks, Contacts, History, Calendar, Messages (on by default); Music, Images, Source files, Plist files (off by default) |
| Default Order | `Sort by Last Used, Last Modified (More Predictable)` |
| Result Limit | `40` |

So the file-search keyword set is fixed and small — `open`, `find`, `in`,
`tags` — and each keyword selects a **dedicated** result set rather than
filtering the default results. The `'` prefix and the empty-box spacebar are
two spellings of the same Quick File Search mode. The Don't Show list is a
filter the pane says applies when "using the open, reveal and inside keywords".

Required behaviour (implemented in `ralfred`): `'`, `open `, `find ` and `in `
are recognised by the router; `open`/`find` attach `openfile:` / `revealfile:`
action tags to each hit; `in` uses the opt-in Spotlight content path; `tags`
reports explicitly that the scoped index stores names only instead of returning
a silent empty list. A keyword requires a following space, so the bare words
remain ordinary queries. Result Limit 40 governs file search; `ralfred`'s own
default-result cap (20 by default) is a separate setting and a different number.

**Search scope and index freshness.** Two distinct scope settings exist and must
not be confused (Alfred's own pane is the first kind; the file index is the
second):

- *Application scope* — where `.app` bundles are looked up for bare queries
  (Alfred: "Search Scope: macOS Applications folder" plus the folder list). In
  `ralfred` this is `default_scope`.
- *File search folders* — the roots the file index walks for `'`, `open`,
  `find` and `in`. In `ralfred` this is `file_roots`, which defaults to
  `~/Documents`, `~/Downloads` and `~/Desktop` when unset or empty. A folder
  added to the *application* scope must not be expected to appear in file
  search, and vice versa.

The index must refresh itself: it re-reads the root list from configuration on
every rebuild, so a folder added or removed takes effect on the next rebuild
**without restarting the application**. The rebuild is a poll (about 45 s in a
long-running process, each attempt under a walk budget), not an event-driven
watcher, so "immediately" means "on the next cycle"; the first query in a fresh
process builds synchronously under a short budget (1.5 s) so a one-shot CLI
query always sees the current configuration. A walk that blocks (a dead network
mount) abandons that attempt and keeps the previous index rather than hanging
the UI. **[C]**

### 5.3 Web search [O]

Shipped/per-user configuration observed as
`preferences/features/websearch/prefs.plist` with a `customSites` dictionary
keyed by UUID, each value `{text, enabled, url, keyword}`, where `url` contains
the `{query}` placeholder, e.g. a site whose url is
`https://github.com/search?q={query}&type=repositories`. Per-engine keyword files
exist under `preferences/features/websearch/<engine>/prefs.plist` holding a
single `keyword` key (observed values on this install: google `gg`, bing `bi`,
youtube `yt`, gtranslate `tr`, dictionary `df`).

Required behaviour: typing `<keyword> <text>` opens the site's URL with the text
percent-encoded into `{query}`; custom sites come from configuration; an
`enabled: false` site never fires.

### 5.4 Calculator [O]

Preference keys: `features.calculator.{enabled, advanced, inputType, outputType,
inputIgnoreThousandsDelimiter, outputShowThousandsDelimiter}`.

Required behaviour: unit-aware arithmetic evaluated inline in the query. Supported
operations: `+ - * / ^ %`, parentheses, `sqrt()`, and unit conversion between
compatible units (data sizes, lengths, masses, temperatures, durations).
Unit conversion is expressed as `<value> <from> to <to>`, e.g. `1gb to mb`.
Conversion between incompatible dimensions must produce no result rather than a
wrong one.

### 5.5 Snippets [O]

`Databases/snippets.alfdb` schema:
`snippets(uid, name, keyword, snippet, snippetrtf BLOB, collection, autoexpand
BOOLEAN, ignoredynamicplaceholders BOOLEAN)` and
`triggers(keyword, triggeruid, workflowuid)`.

Snippet text supports a dynamic-placeholder grammar, observed as these literal
forms:

- `{clipboard}`, `{clipboard:N}` (history entry N), and case/normalisation
  suffixes `{clipboard:trim}`, `{clipboard:uppercase}`, `{clipboard:lowercase}`,
  `{clipboard:capitals}`; the combined form `{clipboard:N.trim}`.
- `{date}` / `{datetime}` with offsets and formats: `{date +1d}`,
  `{date -1d:full}`, `{date +2y +3M +1d:EEEE}`, `{date -10y:y}`,
  `{datetime +1h}`, `{datetime -1M}`, `{datetime +1337h}`; named styles
  `short|medium|long|full`; `{isodatetime}`.
- `{time:full}`.
- `{random:…}` with ranges and choices: `{random:1..10}`, `{random:..}`,
  `{random:..-1}`, `{random:0..}`, `{random:apple,banana,pear}`,
  `{random:UUID}`.
- `{cursor}` — marks where the caret lands after expansion.
- `{snippet:<keyword>}` — nested snippet expansion.

Required behaviour: a snippet with `autoexpand` true expands when its keyword is
typed in any application, subject to per-application exclusions, and expansion
must not recurse infinitely on self-referential snippets. A snippet with
`ignoredynamicplaceholders` true is inserted literally. Snippets are searchable
by keyword from the panel.

### 5.6 Clipboard history [O]

`Databases/clipboard.alfdb` schema:
`clipboard(item, ts, app, apppath, dataType integer, dataHash)`.
Observed statements show type-filtered deletion by age, lookup by `dataHash`,
and a fixed-length text projection (`substr(item, 0, ?)`) for the history
listing, with both a `LIKE` query and an application-defined `ALFRED_UNICODE_MATCH`
function used for search [O].

Required behaviour: a poller records new clipboard text with a timestamp, the
frontmost application's name and path; identical content (same `dataHash`) must
not be duplicated; history is capped by count and by persistence window;
searchable from the panel; selecting an entry restores it to the system
clipboard. Preference keys observed: `features.clipboard.{limit, persistFor,
moveToTopOnUse, ignoreConcealed, ignoreAutoGenerated, clearKeyword,
clearKeywordEnabled, snippetKeyword, viewerKeyword, autoPaste, blacklist,
unicodeMatching}`. Clearing must respect count and keyword forms.

### 5.7 Universal Actions [O]

The identifier family observed in the shipped binary is `alfred.action.*`. The
complete observed set:

```
alfred.action.addtobuffer            alfred.action.copy
alfred.action.browsefolder           alfred.action.copyfile
alfred.action.copyreplace            alfred.action.copypath
alfred.action.delete                 alfred.action.getinfo
alfred.action.move                   alfred.action.movereplace
alfred.action.open                   alfred.action.openterminalhere
alfred.action.openwith               alfred.action.recent
alfred.action.revealinfinder         alfred.action.showaslargetype
alfred.action.showpackagecontents
alfred.action.text.copyasplaintext   alfred.action.text.extracturls
alfred.action.text.openindictionary  alfred.action.text.pasteasplaintext
alfred.action.text.saveassnippet     alfred.action.text.searchwithfinder
alfred.action.text.useinalfred
alfred.action.url.open               alfred.action.url.openin
alfred.action.url.usedomainfromurl
alfred.action.viewinalfred
alfred.action.mixed.email            alfred.action.mixed.emailto
alfred.action.dynamic.actiontrigger.
alfred.action.dynamic.customwebsearch.
alfred.action.dynamic.keywordinput.
alfred.action.dynamic.scriptfilter.
alfred.action.dynamic.systemwebsearch.
alfred.action.trigger.
```

The final group are *dynamic* (parameterised by suffix) and are template forms,
not fixed identifiers.

Required behaviour: when a result is selected and the actions gesture fires, the
tool offers exactly the applicable subset for the selection's kind (file, url,
text), driven by tab/right-arrow; choosing an action executes it and closes the
panel. Applicability is determined by kind: file-only actions must not appear for
text selections and vice versa.

### 5.8 Large Type [O]

Preference keys: `appearance.largetype.{font, style, autohide, showOnMouseScreen}`.

Required behaviour: display the selection's text as large type on a full-screen
overlay; Escape dismisses it; auto-hide respects the configured delay; font and
style come from configuration.

### 5.9 Previews / Quick Look [O]

Preference keys: `features.filesearch.preview.enabled`,
`features.filesearch.quicklook.showWithShift`. Decoded selectors include
`showPreviewForURL:delegate:`, `updatePreviewWithURL:`, `generateQuicklookURL`,
`toggleQuickLook` and `previewPanel:handleEvent:`.

Required behaviour: preview the highlighted file with the system quick-look
facility; if the facility is unavailable, fall back to rasterising the item's
icon; clipboard text is materialised to a temporary file so it too can be
previewed. Preview must never block the panel indefinitely.

### 5.10 Dictionary, Contacts, Music, 1Password, Bookmarks [O]

Each of these is a keyword feature with a configured keyword and an external
back end:

| Feature | Keyword (observed literals) | Back end |
|---|---|---|
| Dictionary | `define`, `dictionary`, `spell` | system dictionary lookup |
| Contacts | `contacts` | address-book query, then call/email actions |
| Music / iTunes | `music`, `itunes` | media transport + library search |
| 1Password | `1password` | `onepassword://` URL schemes |
| Web Bookmarks | `bookmarks` | browser bookmark stores |

Required behaviour: a query after the keyword returns matching entries; the
action opens the item. Each back end is an external dependency, so each must have
a deterministic fixture mode used by tests, and a bounded timeout when the real
back end is invoked (a system query that hangs must not hang the tool).

### 5.11 System actions [O]

Observed keyword literals: `eject`, `ejectall`, `emptytrash`, `forcequit`,
`hide`, `lock`, `logout`, `restart`, `screensaver`, `shutdown`, `sleep`, `quit`,
`quitall`, `trash`, `preferences`, `help`. Preference keys observed under
`features.system.*`.

Required behaviour: each verb performs its system action; the set of keywords
must not collide with one another or with a workflow's keyword (see §8.1).

### 5.12 Themes [O]

**Appearance pane, observed in the running application [O]** (Alfred →
Appearance). The shipped theme list on this install is: *Alfred, Alfred Dark,
Alfred Modern, Alfred Modern Dark, Alfred Classic, Alfred macOS, Alfred macOS
Dark, Frosty Teal, Large Avenir, High Contrast Light* — ten rows, each
labelled "Alfred Default Theme". The pane shows a **live preview panel** for
the selected theme rather than a static swatch, and that preview is where the
result-row anatomy becomes visible: a query row at the top carrying the typed
text plus a mode indicator (the hat icon), then result rows each with an icon,
a **title**, a **subtitle** (for an application result this is its path, e.g.
`/Applications/Mission Control.app`), and a right-aligned **⌘N** shortcut hint
for the first nine rows. The currently selected row is highlighted with the
theme's `result.backgroundSelected` colour and shows a **↵** (return)
affordance at its right edge; the scrollbar is drawn in
`scrollbar.color`/`thickness`. This is the concrete instantiation of the
`result`/`search`/`scrollbar` groups in the schema below.

`.alfredappearance` is JSON whose root is `{"alfredtheme": {…}}`. The observed
schema, complete across the nine `.alfredappearance` files found under the
framework resources. (The Appearance pane lists ten rows, one more than the
nine files observed on disk; the surplus is unexplained and is recorded as an
open discrepancy in §10, not asserted here.)

```jsonc
{"alfredtheme": {
  "name": "…",
  "credit": "…",                      // optional; observed in 2 shipped files
  "visualEffectMode": <int>,          // observed values 1 and 2
  "window":   {"color","borderColor","paddingHorizontal","paddingVertical",
               "borderPadding","roundness","blur","width"},
  "search":   {"background","backgroundSelected","text":{"color","colorSelected","font","size"},
               "paddingHorizontal","paddingVertical","roundness","spacing"},
  "result":   {"backgroundSelected","iconSize","iconPaddingHorizontal",
               "paddingVertical","roundness","textSpacing",
               "text":{"color","colorSelected","font","size"},
               "subtext":{ … }, "shortcut":{ … }},
  "separator":{"color","thickness"},
  "scrollbar":{"color","thickness"}
}}
```

Colours are `#RRGGBB` or `#AARRGGBB` hex strings (alpha **first** in the 8-digit
form), always uppercase in the shipped files, with alphas spanning `#00000000`
to `#FFFFFFFF`. Observed theme names: Alfred, Alfred Classic, Alfred Dark,
Alfred Modern, Alfred Modern Dark, Alfred macOS, Alfred macOS Dark, Frosty Teal,
High Contrast, Large Avenir [O].

Two cautions established against the real files, both of which the loader must
survive:

- `search.paddingHorizontal` and `search.roundness` are present **only** in the
  Modern themes, so every field must be optional with a sane default.
- `Alfred Classic.alfredappearance` is JSON with a trailing comma and therefore
  does **not** parse. It must be skipped with a report while the other nine load.
  Negative values occur too (`window.roundness` is -41 in High Contrast) and are
  preserved rather than clamped.

The binary also carries the diagnostics "Theme not found, Falling back to default
theme", "Unable to load default theme", "Unable to create json for theme",
"Incorrect theme file", "Theme number out of range" [O].

Required behaviour: themes load from JSON in this schema; an unreadable or
invalid theme falls back to the built-in default and reports the fallback rather
than failing; a theme can be selected by name; alpha in the colour is honoured.

### 5.13 Migration [O]

A legacy `Alfred.alfredpreferences` tree, `prefs.json` pointer file, and
`Databases/*.alfdb` stores exist on disk. Required behaviour: `ralfred migrate`
imports workflows and the snippet/clipboard/url-history/knowledge stores
read-only (never mutating the source), is idempotent (a second run adds nothing),
and supports `--dry-run` to report what would change without changing it.

### 5.14 Remote [O]

Observed action identifiers: `remote.alfred.{showalfred, defaultwebsearch,
runworkflow, openurl, launchfile, clipboardsnippet, dispatchkeycombo,
itunescommand, openmacossetting, runapplescript, runscript, runterminalcommand,
systemcommand, alfredpreferences}`; a distributed-notification channel
`alfred.distributednotification.remote.{command,control}` was observed [O]. The
wire protocol was **not** examined (no network capture was performed).

Given the unexamined protocol, `ralfred` does not implement a remote server.
This is a non-goal (§10), recorded with its evidence gap.

---

## 6. Workflows

### 6.1 Bundle format [O]

A workflow is a zip (or a directory) whose `info.plist` has these top-level keys:

| Key | Type | Meaning |
|---|---|---|
| `bundleid` | string | workflow identifier |
| `name`, `description`, `readme`, `createdby`, `webaddress`, `version` | string | metadata |
| `disabled` | bool | workflow disabled |
| `objects` | array | `{type, uid, version, config, …}` |
| `connections` | dict | source object uid → array of edge objects |
| `uidata` | dict | per-uid `xpos`, `ypos`, `note` (canvas layout) |
| `userconfigurationconfig`, `variablesdontexport` | array | user-facing configuration |

Each edge object observed has exactly:
`{"destinationuid", "modifiers", "modifiersubtext", "vitoclose"}`, where
`modifiers` is an integer modifier mask and `vitoclose` is a boolean [O].

A referenced output uid that no longer exists must be dropped from the graph and
reported, mirroring the observed diagnostic that a defunct connection is tidied
because its output uid is missing [O].

### 6.2 Object types [O]

The observed `type` strings, grouped:

- inputs: `alfred.workflow.input.{keyword, scriptfilter, filefilter, listfilter,
  dictionaryfilter, snippetfilter, runningappsfilter, calculatorfilter,
  confacfilter, webbookmarks, 1password, internalsearch, systemhideorquit}`
- actions: `alfred.workflow.action.{openurl, openfile, launchfiles, revealfile,
  browseinterminal, browseinalfred, actioninalfred, applescript, script,
  systemcommand, systemwebsearch, terminalcommand, buffer, itunescommand}`
- outputs: `alfred.workflow.output.{clipboard, notification, largetype, speak,
  playsound, writefile, callexternaltrigger, dispatchkeycombo, script}`
- triggers: `alfred.workflow.trigger.{hotkey, external, universalaction, snippet,
  contact, action, fallback, remote}`
- utilities: `alfred.workflow.utility.{argument, conditional, junction, filter,
  delay, debug, dialog, expression, file, hidealfred, joinargs, json, random,
  replace, showalfred, split, transform}`
- automations: `alfred.workflow.automation.{runshortcut, task}`

### 6.3 Execution model

Required behaviour: an input object produces either a result list (filter inputs)
or an argument which is passed along outgoing connections; action objects consume
the argument and perform an effect; output objects emit. Connections are walked
breadth-first with a visited set so that cycles terminate. Subtext and modifiers
on an edge select alternate destinations.

`ralfred` must implement at minimum: script filter inputs, keyword inputs, the
`openurl`, `openfile`, `launchfiles`, `revealfile` and `script`-family actions,
`notification` and `clipboard` outputs, and the `junction`/`conditional`
utilities; unhandled types behave as pass-through so that a graph containing them
still runs its supported path, and the omission is reported rather than silently
dropped. **[C]** for the pass-through policy; the type list is [O].

### 6.4 Script filter — configuration keys [O]

Observed in a shipped workflow's `config` dict:

| Key | Meaning |
|---|---|
| `keyword` | trigger keyword |
| `title`, `subtext`, `runningsubtext` | row text |
| `script`, `scriptfile` | inline script or path |
| `type` | script language index (0 observed) |
| `scriptargtype` | how the argument is passed (1 observed) |
| `argumenttype` | argument form (0 and 2 observed) |
| `escaping` | escaping mode (102 observed) |
| `withspace` | whether a space is required after the keyword |
| `alfredfiltersresults` | whether Alfred fuzzy-filters the returned items |
| `queuemode`, `queuedelaymode`, `queuedelaycustom`, `queuedelayimmediatelyinitially` | re-run queueing policy |

### 6.5 Script filter — invocation [O]

The script is executed with the query available to it, and with the environment
variables of §6.7. Alfred passes the query as an argument and/or through the
environment according to `scriptargtype`/`argumenttype`.

### 6.6 Script filter — output contract [O]

The script prints either JSON or XML on stdout.

**JSON**

```jsonc
{
  "items": [ {
      "uid": "…",            // stable identity for ranking/selection
      "type": "file",        // optional row kind (e.g. "file")
      "title": "…",
      "subtitle": "…",
      "arg": "…",            // payload passed to the next object
      "valid": true,         // false ⇒ row is not actionable
      "autocomplete": "…",
      "match": "…",          // text used for filtering instead of title
      "icon": { "type": "fileicon|filetype|default", "path": "…" },
      "quicklookurl": "…",
      "text": { "copy": "…", "largetype": "…" },
      "mods": { "cmd": {"valid":bool,"arg":"…","subtitle":"…","icon":{…}}, "alt": {…} }
  } ],
  "variables": { "name": "value" },   // exported to later objects in the session
  "rerun": 1.0                        // 0.1–5.0 seconds
}
```

**XML (legacy, equivalent meaning)**

```xml
<?xml version="1.0"?>
<output>
  <rerun>1</rerun>
  <variables><variable name="counter">1</variable></variables>
  <items>
    <item arg="…">
      <title>…</title>
      <subtitle>…</subtitle>
    </item>
  </items>
</output>
```

**Required parse/serialise behaviour:**

- JSON must be accepted when it is an object; the XML/plist form must have an
  `alfredworkflow` root dictionary object [O: the diagnostic "JSON needs to have
  an 'alfredworkflow' root dictionary object"].
- Missing expected keys must be reported, not silently accepted: the observed
  diagnostic names `items`, `variables` and `rerunafter` as the expected keys [O].
- A parse failure must surface the offending fragment and position; observed
  diagnostics for JSON and XML include the row and column [O].
- `rerun` is only honoured when it lies in 0.1–5.0 seconds, and the script is
  re-run only while the filter is still active and the query has not changed [O].
- `variables` returned by a filter are passed back into the same script on a
  later run in the same session, and are forwarded to downstream objects [O].

### 6.7 Environment contract for scripts [O]

These variables are exported to workflow scripts (full observed set):

```
alfred_version              alfred_version_build
alfred_workflow_bundleid    alfred_workflow_cache
alfred_workflow_data        alfred_workflow_description
alfred_workflow_keyword     alfred_workflow_name
alfred_workflow_uid         alfred_workflow_version
alfred_preferences          alfred_preferences_localhash
alfred_debug
alfred_theme                alfred_theme_background
alfred_theme_selection_background   alfred_theme_subtext
alfred_theme_window_width
```

Types observed alongside them (workflow-variable type names):
`alfred_string`, `alfred_bool`, `alfred_integer`, `alfred_double`,
`alfred_array`, `alfred_dictionary`, `alfred_data`; plus `alfred_script` [O].

Required behaviour: `ralfred` exports `ralfred_`-prefixed equivalents for its own
identity, **and** exports the `alfred_*` names above when running a workflow
authored for Alfred, so that third-party workflows interoperate. `ralfred` is
the value of the version variables it owns. **[C]** for the dual export; the
name list is [O].

### 6.8 Workflow file/asset paths [O]

Observed directory names: `Workflow Data` and a per-workflow cache; scripts
receive both. `ralfred` maps these to its own data root while honouring the
variable names in §6.7. **[C]**

---

## 7. Configuration and on-disk formats

### 7.1 Root pointer [O]

`<Application Support>/Alfred/prefs.json` contains exactly two keys: `current`
(absolute path of the active `.alfredpreferences` tree) and `localhash` (a
per-machine identifier). `ralfred` keeps the same pointer file so it can locate a
legacy tree, and additionally supports its own JSON configuration.

### 7.2 Preference tree [O]

```
Alfred.alfredpreferences/
  preferences/
    features/<area>/prefs.plist          # per-feature settings
    local/<localhash>/
      appearance/prefs.plist
      features/defaultresults/prefs.plist
  remote/{pages,images,local}
```

Preference keys are dot-delimited and namespaced by area. Observed families
include (non-exhaustive, ~550 keys): `features.defaultresults.*`,
`features.filesearch.*`, `features.clipboard.*`, `features.websearch.*`,
`features.webbookmarks.*`, `features.snippets.*`, `features.system.*`,
`features.calculator.*`, `features.contacts.*`, `features.1password.*`,
`features.universalactions.*`, `appearance.largetype.*`, `appearance.options.*`,
`workflows.*`, `remote.*`, `advanced.*`.

Required behaviour: `ralfred` config is a JSON document that can express the
user-visible settings for every feature it implements; reading a legacy plist
tree yields the same effective settings. Unknown keys are preserved on write
round-trip (no destructive rewrite). **[C]**

### 7.3 Databases [O]

`ralfred` owns stores under its data root with schemas compatible with the
observed Alfred schemas, so that migration and interoperation are possible:

- `knowledge.alfdb` — `knowledge(item, keyword, ts, hidden)`,
  `latching(item, keyword, strong, ts)`, `other(type, uid, data, context, ts)`,
  `recentdocs(path, bundle, ts)`
- `clipboard.alfdb` — `clipboard(item, ts, app, apppath, dataType, dataHash)`
- `snippets.alfdb` — `snippets(uid, name, keyword, snippet, snippetrtf, collection,
  autoexpand, ignoredynamicplaceholders)`, `triggers(keyword, triggeruid, workflowuid)`
- `urlhistory.alfdb` — `history(item, flatItem, ts)`
- `filecache.alfdb` — `files(path, fileName, name, nameSearch, nameSplit, nameChars,
  lastUsed, keywords, altnames, filetype)`
- `actions.alfdb` — `usedActions(actionTypeUid, fileType, ts)`

Reindexing must be additive: an existing user's data must survive a schema
upgrade, and stores must be created on demand rather than preventing the tool
from starting.

---

## 8. Edge cases (normative)

These are the failure and boundary behaviours the implementation must satisfy.

**8.1 Keyword conflicts.** If a feature keyword and a workflow keyword collide,
the query must not silently run one of them; the conflict is reported. [O]

**8.2 Missing workflow graph targets.** An edge whose `destinationuid` names no
object is dropped and reported; the rest of the graph still runs. [O]

**8.3 Malformed workflow bundles.** A corrupted or invalid workflow must be
rejected with a clear error and must not be partially installed. [O]

**8.4 Malformed script-filter output.** Non-JSON, non-XML output, JSON missing
`items`, and XML without the expected root must each produce a distinct,
actionable error naming the problem and the offending text. [O]

**8.5 Invalid rerun interval.** A `rerun` outside 0.1–5.0 is not honoured; the
filter does not busy-loop. [O]

**8.6 Empty and whitespace queries.** An empty query shows default results (or
nothing), never a crash; leading/trailing whitespace is trimmed before keyword
dispatch.

**8.7 Missing paths.** A configured scope root, preview target, or script file
that does not exist is skipped and reported; the tool keeps running. [O:
diagnostics such as "File '%@' not found", "Script '%@' not found"]

**8.8 Unreadable theme.** Falls back to the default theme and reports it (§5.12).

**8.9 Too many grid items.** A grid layout request exceeding its capacity is
refused with a diagnostic rather than truncating silently. [O: "There are too
many items for a %ldx%ld grid."]

**8.10 Too many clipboard types.** A clipboard change with no recognised type is
ignored rather than stored. [O: "Clipboard types have been empty for too long,
ignoring change"]

**8.11 Self-referential snippet.** A snippet whose expansion contains its own
keyword must not expand recursively without bound.

**8.12 Duplicate workflows.** An archive and its already-unpacked directory are
one workflow, listed once.

**8.13 Concurrent configuration writes.** Two writers must not corrupt the
configuration file: writes are atomic (write-temp-then-rename) and read-modify-
write cycles tolerate a concurrent change by re-reading. **[C]**

**8.14 Blocking external back ends.** Any system query (Spotlight, AppleScript,
media, contacts, bookmarks, preview, quick-look) must be bounded; exceeding the
bound degrades to no result rather than hanging the UI.

**8.15 Unavailable permissions.** When the OS denies a capability (accessibility,
automation, contacts), the feature reports the denial and the rest of the tool
continues to work.

**8.16 Stale blur events.** A focus-loss event delivered while the panel is being
re-shown must not hide the freshly summoned panel (§3.1).

**8.17 Duplicate hotkey events.** A single physical hotkey press that yields both
a press and a release event must toggle the panel exactly once (§4.2).

**8.18 Empty clipboard history.** Searching and clearing an empty history is a
no-op, not an error.

**8.19 Non-UTF-8 / large clipboard content.** Binary or oversized clipboard
payloads are handled without panicking; text output is truncated for display
while the stored item remains intact.

**8.20 Interrupted migration.** A migration that fails midway leaves the source
untouched and the destination in a consistent state; re-running completes it.

**8.21 Fallback boundaries.** The fallback band runs only after every other band
has produced nothing. It is suppressed for a multi-token query (so a mistyped
feature keyword does not become a web search) and for any query that named a
file-search mode (`'`, `open`, `find`, `in`, `tags`), so the mode prefix is
never echoed into a fallback URL. A file-search keyword whose back end is
unavailable (`tags` with no tag metadata, `in` with content search switched
off) reports that fact in one row rather than returning a silent empty list.
**[C]**

**8.22 Unrecognised option.** A long option that the command line does not
define must be reported as an unknown option and must not be treated as a
search query: `ralfred --gui` is a mistyped invocation, and answering it with a
web search for the literal text `--gui` hides the mistake. Exit status is
non-zero. **[C]**

**8.23 Partial configuration.** A configuration file that specifies only some
keys must load: the keys present are honoured and the keys absent take their
defaults. A configuration that fails to load *silently* is worse than one that
fails loudly — the user's settings would appear to reset for no visible reason.
Name matching in the file index is a case-insensitive substring test on the
file name: a prefix match outranks a match after a name separator
(`_`, `-`, `.`, space, `(`), which outranks a match in the middle of a word;
recency breaks ties. A multi-token query is matched as one literal substring,
so it only hits when the file name contains it verbatim. **[C]**

---

## 9. Non-functional requirements

1. **Startup:** the panel must appear without waiting for a file index build;
   indexing happens in the background. **[C]**
2. **Per-keystroke latency:** query handling must not perform unbounded
   synchronous I/O on the UI thread. **[C]**
3. **No panics:** user input, file contents and workflow output must never panic
   the process. Fallible parsing returns errors.
4. **Determinism for tests:** every external back end has a fixture path, and the
   data root is relocatable via `RALFRED_HOME`, so the full suite runs without
   touching the user's real data or performing real actions
   (`RALFRED_NOACT=1`). **[C]**
5. **Clean-room hygiene:** no Alfred binary, asset, nib, theme file, icon or
   licence material is redistributed; only interface facts are used. **[C]**

---

## 10. Non-goals and recorded divergences

| Item | Reason |
|---|---|
| Powerpack licence / activation | Deliberately not reproduced. `ralfred` is free and has no key validation. [O: the licence machinery exists in the target] |
| Alfred Remote server wire protocol | Not examined (no network capture performed); identifiers observed only. |
| AppleScript dictionary (`NSScriptCommand` host) | Requires being an AppleScript-capable app; `ralfred`'s equivalent is its CLI and URL scheme. |
| Control-flow-exact ranking and fuzzy scoring | No decompiler authority was available, so the exact algorithms are unknown; `ralfred` specifies its own deterministic ranking (§3.4) that satisfies the observable ordering properties. |
| Gallery / Assistant network features | Depend on Alfred's online service; out of scope. |
| Exact panel pixel geometry and animations | Not established for the target; `ralfred` uses its own theme-driven geometry (§3.5). |
| Appearance pane lists ten themes, nine `.alfredappearance` files found on disk | The surplus row is unexplained; the extra name may come from a built-in default or a user-installed theme whose file lives outside the framework resources. Not resolved, so the nine-file schema in §5.12 is what is asserted. |
| Fallback band narrowed to single-token queries and suppressed for file-search modes | The pane states fallbacks appear "when there are no results" without qualification; the narrowing is a deliberate `ralfred` decision to avoid converting a mistyped keyword into a web search (§8.21), not an observed Alfred behaviour. |

---

## 11. Conformance checklist

An implementation conforms when all of the following hold and are covered by
automated tests:

1. Workflow `info.plist` parses, `objects`/`connections`/`uidata` round-trip, and
   missing connection targets are dropped and reported (§6.1–6.2, §8.2).
2. A script filter that emits each documented JSON and XML shape is parsed
   correctly, including `variables` and `rerun`; malformed shapes produce the
   specified errors (§6.6, §8.4, §8.5).
3. The `alfred_*` environment contract is exported to scripts (§6.7).
4. `knowledge`, `clipboard`, `snippets`, `urlhistory`, `filecache` and `actions`
   stores exist with the specified columns, and ranking orders by recency with
   hidden items excluded (§3.4, §7.3).
5. Snippet placeholders expand for clipboard, date/datetime/time, random and
   cursor forms, and stop at a recursion bound (§5.5, §8.11).
6. Calculator evaluates arithmetic and converts compatible units, and refuses
   incompatible conversions (§5.4).
7. Web-search keywords substitute percent-encoded text into `{query}`, and
   disabled sites do not fire (§5.3).
8. The `alfred.action.*` set is exposed with correct kind-based applicability
   (§5.7).
9. Themes parse from the documented JSON schema and fall back on error (§5.12).
10. Migration is read-only against the source and idempotent (§5.13, §8.20).
11. Every §8 edge case has a test that exercises it.
12. The full suite passes from a clean checkout with no network and no user state.
13. The file-search Quick prefix (`'`) and the four file keywords (`open`,
    `find`, `in`, `tags`) each select a dedicated result set; a keyword applies
    only as a whole word; and a keyword whose back end is unavailable
    (`tags` without tag metadata, `in` with content search off) reports that
    fact in one row instead of returning a silent empty list (§5.2, §8.21).
14. The fallback band runs only when every other band produced nothing, and is
    suppressed for multi-token queries and for queries that named a file-search
    mode (§3.2 step 5, §5.1, §8.21).
15. The file index re-reads its root list on every rebuild: a folder added to
    `file_roots` becomes searchable by the next query and a removed folder
    disappears again, in both cases without restarting the application
    (§5.2).
16. A configuration file specifying only some keys loads with defaults for the
    rest, and file-name matching finds a keyword anywhere in the name —
    prefix, after a separator, or mid-word — case-insensitively (§5.2, §8.23).
