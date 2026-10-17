# REVERSE-ENGINEERING NOTES — Alfred 5.8 (evidence-based, 2026-10)

> Clean-room basis: behavior + public data formats only. No code/assets/themes copied.
> The normative, per-requirement specification derived from this evidence is
> [`SPEC.md`](../SPEC.md); this file is the short orientation note.

## Bundle anatomy
- Main executable: thin ObjC launcher (190/202 KB per arch). All logic in
  `Alfred Framework.framework` (11 MB, ~780 `Alfred*` names; the metadata reader
  decodes 279 non-metaclass classes with method records).
- Universal Mach-O (arm64+x86_64), min macOS 11, pure Cocoa/ObjC.
- Bundle version: `CFBundleShortVersionString` 5.8, `CFBundleVersion` 2348.
- URL scheme `alfred://` (handler `runtrigger`, `customsearch`, `gallery`);
  AppleScript dictionary `Contents/Resources/Alfred.sdef`.

## Feature modules (from class graph)
default search (apps/docs), file search, web search, snippets + autoexpand,
clipboard history (+persistent), calculator (unit-aware), dictionary, contacts,
1Password, iTunes, bookmarks, system actions, terminal/open, universal actions,
large type, previews, workflow engine (script filters + connections graph),
knowledge DB (usage ranking), hotkey manager, remote server, help assistant.

## Data formats (verified from live files)
- `prefs.json` -> current prefs dir + localhash.
- `Alfred.alfredpreferences/preferences/{features/<area>,local/<hash>,workflows}/…`
  — plist trees. Workflows live at `preferences/workflows/<uid>/info.plist`
  (observed layout string `workflows/%@/info.plist`).
- `*.alfdb` SQLite (schemas in SPEC.md §7.3). filecache/clipboard/snippets/
  urlhistory/knowledge/actions.
- Workflow: `.alfredworkflow` = zip(info.plist) with `objects[]` {config, uid,
  type} and top-level `connections{uid: [{destinationuid, modifiers,
  modifiersubtext, vitoclose}]}`.
- Script filter: query in via argv/env; output may be **JSON**
  (`{"items":[…],"variables":{…},"rerun":0.1–5.0}`) or the legacy **XML**
  (`<output><rerun/><variables/><items><item arg="…">…`).
- `alfred_*` environment contract exported to scripts (see SPEC.md §6.7).
- `.alfredappearance` theme JSON: `{"alfredtheme": {name, credit?,
  visualEffectMode, window, search, result, separator, scrollbar}}`.
- Default results scope: configurable folder list (verified in user prefs).

## Preference panes (observed directly in the running app)
- **Default Results**: types are bucketed — Applications (`Options…`),
  Essentials (Contacts, System Settings), Extras (Folders, Documents, Text
  Files, Images, Archives, AppleScripts, `Advanced…`), "Unintelligent"
  (`Search all file types`, warns it slows the default results), and
  Fallbacks (`Only show fallbacks when there are no results`). Pane note: by
  default Alfred falls back to **Google, Amazon and Wikipedia**, and
  `[spacebar]` on an empty box enters file search.
- **File Search** (tabs: Search, Navigation, Buffer, Advanced): Quick File
  Search prefix `'` or spacebar; keywords **`open`** (opening), **`find`**
  (revealing), **`in`** (inside files), **`tags`** (file tags); a Don't Show
  filter list (Emails/Bookmarks/Contacts/History/Calendar/Messages on);
  Default Order "Sort by Last Used, Last Modified (More Predictable)";
  Result Limit **40**.
- **Appearance**: live preview panel (not a static swatch) — this is where row
  anatomy is readable: query row with hat mode indicator, rows of
  icon + title + subtitle (app subtitle = bundle path), right-aligned ⌘N hints
  on the first nine rows, ↵ affordance on the selected row. Theme list: Alfred,
  Alfred Dark, Alfred Modern, Alfred Modern Dark, Alfred Classic, Alfred macOS,
  Alfred macOS Dark, Frosty Teal, Large Avenir, High Contrast Light — ten rows
  against the nine `.alfredappearance` files observed on disk (unexplained;
  recorded in SPEC.md §10).

## Ranking (observed behavior, knowledge.alfdb)
- knowledge(item, keyword, ts, hidden): usage recency per query
- latching(item, keyword, strong): sticky items
- other(type, uid, data, context, ts); recentdocs(path, bundle, ts)
- recency-weighted score; hidden items excluded.

## License
- "Alfred 5 Powerpack License" (trial + paid, Powerpack feature gate) —
  NOT reimplemented. ralfred is free; no key validation.
