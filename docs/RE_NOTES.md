# REVERSE-ENGINEERING NOTES — Alfred 5.8 (evidence-based, 2026-10)

> Clean-room basis: behavior + public data formats only. No code/assets/themes copied.

## Bundle anatomy
- Main executable: thin ObjC launcher (190/202 KB per arch). All logic in
  `Alfred Framework.framework` (11 MB, 731 ObjC classes, unstripped).
- Universal Mach-O (arm64+x86_64), min macOS 11, pure Cocoa/ObjC.

## Feature modules (from class graph)
default search (apps/docs), file search, web search, snippets + autoexpand,
clipboard history (+persistent), calculator (unit-aware), dictionary, contacts,
1Password, iTunes, bookmarks, system actions, terminal/open, universal actions,
large type, previews, workflow engine (script filters + connections graph),
knowledge DB (usage ranking), hotkey manager, remote server, help assistant.

## Data formats (verified from live files)
- `prefs.json` -> current prefs dir + localhash.
- `Alfred.alfredpreferences/preferences/{global,local/<hash>}/...` — plist trees.
- `*.alfdb` SQLite (schemas dumped in db.rs). filecache/clipboard/snippets/
  urlhistory/knowledge/actions.
- Workflow: `.alfredworkflow` = zip(info.plist) with `objects[]` {config, uid,
  type, connections via `connections{uid: [{destinationuid, modifiers}]}}.
- Script filter JSON in: argv {query, workflowvariables}; JSON out:
  {"items":[{"title","subtitle","arg","uid","autocomplete","valid","icon",...}]}
- Default results scope: configurable folder list (verified in user prefs).

## Ranking (observed behavior, knowledge.alfdb)
- knowledge(item, keyword, ts, hidden): usage recency per query
- latching(item, keyword, strong): sticky items
- recency-weighted score; hidden items excluded.

## License
- "Alfred 5 Powerpack License" (trial + paid, Powerpack feature gate) —
  NOT reimplemented. ralfred is free; no key validation.
