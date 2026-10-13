# 04 — Data and Behavior (integration contracts)

## 1. Preference storage layout (observed, fs + plist + dispatch)

- `~/Library/Application Support/Alfred/prefs.json` — 2 keys only: `current` (absolute path to the active `Alfred.alfredpreferences` tree) and `localhash` (per-Mac identifier). (fs, observed on this machine.)
- `Alfred.alfredpreferences/` — a *package* (UTI `com.runningwithcrayons.alfred.preferences`, "LSTypeIsPackage", plist) with:
  - `preferences/features/<area>/prefs.plist` — per-feature settings (e.g. `websearch/prefs.plist` observed with `customSites` keyed by UUID → `{text, enabled, url, keyword}`).
  - `preferences/local/<hash>/…` — per-Mac overrides (`appearance/prefs.plist`, `features/defaultresults/prefs.plist` observed; scope list example seen).
  - `remote/` — remote-page tree (`pages`, `images`, `local`).
- Live preference keys read from binary strings (`strs`, ~550 distinct `features.*`/`appearance.*`/`search.*`/`workflows.*`/`remote.*` keys; full list in `raw/strings_probe` output captured this session — re-run probe to regenerate): notable families: `features.defaultresults.*` (scope includes/excludes, show* toggles, fallbacks), `features.filesearch.*` (actions/preview/buffer/navigation/resultLimit), `features.clipboard.*` (limit, persistFor, autoExpand, viewer keywords, clear keywords), `features.websearch.urls.*`, `features.webbookmarks.*` (chrome/safari indexing), `features.snippets`, `features.system.*` (eject/quit/hide forcequit keywords), `features.calculator.*`, `features.contacts.*` (fields, email), `features.1password.*`, `features.universalactions.*`, `workflows.categories|palette|tasks.*`, `remote.server*`.
- `preferences.alfdb` exists but is **empty** on this install (observed: 0 tables).
- `alfred.alfdb` — 0 tables on this install; name suggests legacy/placeholder (unknown usage).

## 2. SQLite databases (`Databases/*.alfdb`, observed via sqlite_master)

| file | tables (columns verbatim) |
|---|---|
| `filecache.alfdb` | `files(path, fileName, name, nameSearch, nameSplit, nameChars, lastUsed integer, keywords, altnames, filetype integer)` |
| `knowledge.alfdb` | `knowledge(item, keyword, ts, hidden)`, `latching(item, keyword, strong, ts)`, `other(type, uid, data, context, ts)`, `recentdocs(path, bundle, ts)` |
| `clipboard.alfdb` | `clipboard(item, ts, app, apppath, dataType integer, dataHash)` |
| `snippets.alfdb` | `snippets(uid, name, keyword, snippet, snippetrtf BLOB, collection, autoexpand BOOLEAN, ignoredynamicplaceholders BOOLEAN)`, `triggers(keyword, triggeruid, workflowuid)` |
| `urlhistory.alfdb` | `history(item, flatItem, ts)` |
| `actions.alfdb` | `usedActions(actionTypeUid, fileType, ts)` |

Note: `nameChars` column and `latching`/`knowledge` tables match the fuzzy/typed-queries model referenced by `features.defaultresults` keys (strs) — consistency, not proof of code paths (no disassembly authority).

## 3. Workflow bundle contract (observed)

`.alfredworkflow` = zip (UTI conforms `public.zip-archive`; also `.alfred3workflow|.alfred4workflow|.alfred5workflow`, plist `CFBundleDocumentTypes` + UTI exports).

`info.plist` keys observed (sample: bundled "Script Filter.alfredworkflow"): `bundleid`, `name`, `version`, `readme`, `createdby`, `description`, `disabled`, `connections` (dict keyed by object uid → output uid → list of targets), `objects` (array of `{type, uid, version, config, uidata?}`), `uidata` (per-uid `xpos`/`ypos`/`note`), `userconfigurationconfig` (+ `variablesdontexport`), `variablesdontexport`, per-object `config` dict carrying the UI pane fields. (fs.)

Object `type` values (`strs`, 61): inputs `alfred.workflow.input.{keyword, scriptfilter, filefilter, listfilter, dictionaryfilter, snippetfilter, runningappsfilter, calculatorfilter, confacfilter, webbookmarks, 1password, internalsearch, systemhideorquit}`; actions `alfred.workflow.action.{openurl, openfile, launchfiles, revealfile, browseinterminal, browseinalfred, actioninalfred, applescript, script, systemcommand, systemwebsearch, terminalcommand, buffer, itunescommand}`; outputs `alfred.workflow.output.{clipboard, notification, largetype, speak, playsound, writefile, callexternaltrigger, dispatchkeycombo, script}`; triggers `alfred.workflow.trigger.{hotkey, external, universalaction, snippet, contact, action, fallback, remote}`; utilities `alfred.workflow.utility.{argument, conditional, junction, filter, delay, debug, dialog, expression, file, hidealfred, joinargs, json, random, replace, showalfred, split, transform}`. Class names pair 1:1 with config nibs (doc 02 §3).

Script Filter output contract (observed from bundled example scripts): JSON `items[]` with keys `uid, type ("file"|…), title, subtitle, arg, valid, autocomplete, icon{type: fileicon|filetype|default, path}, quicklookurl, text{copy, largetype, qlpreview?}, mods{cmd|alt|ctrl|fn|shift|alt+cmd… {valid, arg, subtitle, arg, icon?}}`; XML legacy form with `subtitle mod=`, `<mod>`, `<text type=>`, `<quicklookurl>` equivalents. (fs, sample contents — keys documented, not copied as data.)

External triggers: `alfred://runtrigger/<workflow-uid>/<obj-uid>?argument=…` (strs). Custom searches: `alfred://customsearch/<uid>/<name>/<emoji?>/<url>/<keyword>` (5-arg template, strs).

## 4. URL schemes & file types (observed plist + strs)

- `alfred://` registered scheme (`CFBundleURLSchemes`, plist); handler chain `application:openURLs:` → `handleURLCommand:` (disp, exe) → `findAndRunTriggerFromURLPath:` (disp).
- Document types/UTIs: `.alfredpreferences` (package), `.alfredworkflow*` (zip), `.alfredappearance` (conforms `public.json`), `.alfredsnippets`; UTI ids `com.runningwithcrayons.alfred.{preferences,workflow,appearance,snippets}` (plist `UTExportedTypeDeclarations`).
- `dict://{query}` — dictionary-search preview URL form (strs).

## 5. AppleScript sdef (observed, `Contents/Resources/Alfred.sdef`)

Commands: `search`, `action` (param `type text`), `browse`, `run trigger` (params `in workflow`, `with argument`), `reload workflow`, `reveal workflow` (params `configuration`, `details`), `set configuration` (`to value`, `in workflow`, `exportable`), `remove configuration` (`in workflow`), `set theme`. Handler classes in the launcher: `AlfredAppleScriptCommand*` (disp). (plist `NSAppleScriptEnabled: true`.)

## 6. Notifications & distributed notifications (observed strs; semantics inferred)

- App-internal `NSNotification` names in `alfred.*` form (244-name family): e.g. `alfred.didshow`, `alfred.didhide`, `alfred.hide`, `alfred.fallback`, `alfred.darkmode.changed`, `alfred.ipaddressesupdated`, `alfred.flagschanged`, `alfred.buffer.contentschanged`, `alfred.clipboard`, `alfred.cache.imageloaded`, `alfred.contactsquery.results.updated`, `alfred.metadataquery.results.updated`, `alfred.itunes.*`, `alfred.action.*` (universal-action identifiers), `alfred.automation.tasks.*`, `alfred.gallery.updatechecker.updatesavailable`, `alfred.contact.field.*` (contacts field identifiers). Names are observed strings; delivery scope (NS vs distributed) not individually verified.
- Distributed: `alfred.distributednotification` (+ `.remote.command`, `.remote.control`, `.troubleshooting.function[.result]`), managers `AlfredDistributedNotificationManager`, `AlfrediTunesDistributedNotificationManager` (disp) — cross-process control between main app, prefs helper, and the iOS Remote server. **Inferred** use: cross-app coordination only; the wire protocol is unexamined (network capture not performed).
- User notifications: `AlfredUserNotificationManager` (disp) via `NSUserNotificationCenter`; prefs `alfred.server.usernotification`, `notifications.removewhenclicked`; workflow output `notification` object type.
- Preferences-change propagation: `replaceAlfredViewNotification:`, `updateQueryNotification:`, `resizeWindowNotification:`, `distributedUpdateChangedNotification:`, `hideAlfredNotification:` (disp, exe/strs).

## 7. Workflow / runtime behavior facts (observed where tagged)

- Panel view lifecycle: show/hide via `showAlfredWithInput:argument:config:variables[:leftCursor:]` (disp); stack views for drill-ins (observed method names); buffer view toggled by `isBufferVisible`/`setupBufferView` (disp) — clipboard buffer feature (`features.filesearch.buffer.*`, `clearAfterIdle`, `useShift`).
- Search execution: `processSearchField`/`searchTextDidChange` per keystroke (disp) with `updateQueryNotification:` and `resultsUpdated:` callbacks to `AlfredDefaultResultsView` (disp). Queued execution modes exist in workflow inputs (`queuedelaymode`, `queuemode` config keys, observed in workflow plists).
- Remote iOS integration: `remote.*` action types (`remote.alfred.{showalfred, defaultwebsearch, runworkflow, openurl, launchfile, clipboardsnippet, …}`, strs) map to `AlfredRemoteAction*` classes + nib config panes (doc 02); server keywords for restart/stop (strs).
- Gallery: `Gallery/` data dir (fs), `AlfredGalleryWorkflowInstaller` (disp) + `AlfredGalleryDependencies*` (nibs; dev-tools/homebrew dependency checks, Homebrew formulae aux nib — observed strings).
- Usage analytics: `usage.data` (fs), `AlfredPreferencesUsageGraphView` (disp).

## 8. Known gaps / unresolved (see also JSON `gaps`)

1. No disassembly/decompiler authority (Ghidra/Hopper/IDA absent) — control-flow-level behavior (search ranking, file-cache algorithm, fuzzy scoring `nameChars/latching` semantics) is **unknown**; doc claims above are names + data structures only.
2. Runtime panel capture blocked this session (window never surfaced to capture tooling); pixel metrics quoted as `runtime[prev]`.
3. `AlfredTableView.nib` + ~67 framework config nibs undecoded at the 64-document cap.
4. Theme JSON (`.alfredappearance`) schema inferred from `search.*` key names only; no theme file shipped in bundle to validate (unknown).
5. Default hotkey literals (e.g. ⌥Space) unverified; stored per-user.
6. `alfred.alfdb`/`preferences.alfdb` empty here — purpose inferred, unconfirmed.
