# 01 — Tooling and Method

Target: `/Applications/Alfred 5.app`, version **5.8** (`CFBundleShortVersionString`; plist evidence `raw/plist.json`).
Tool: `rea` CLI (repo-local build, `rea@6.4.0`) run as `cd /Volumes/Elements-Mini/GitHub/rea && node scripts/rea.mjs <command> …`.
Evidence root: this directory; raw CLI outputs in `raw/`. Scratch copies were pruned; JSON kept in `raw/` are verbatim tool envelopes.

## Capability survey (rea 6.4.0, this host)

Command: `node scripts/rea.mjs capabilities`

| Provider | Operations | Available |
|---|---|---|
| native-macos | `capture_native_ui_scenario`, `observe_native_calls`, `inspect_macho`, `inspect_plist`, `inspect_signature`, `demangle_swift`, `list_architectures`, `inspect_native_dispatch_metadata`, `observe_native_ui` | yes |
| rea-artifact-graph | `decode_interface_builder`, `inspect_keyed_archive`, `trace_dylib_resolution`, `extract_artifact`, `inspect_artifact`, `inspect_asset_catalog` | yes |
| rea-dotnet-static | managed artifacts | n/a (no PE/CLI target) |
| ghidra (deep analysis) | — | **unavailable** (`not_configured`: `GHIDRA_INSTALL_DIR` unset, no Java, no `analyzeHeadless`) |

`analysis_provider_binding: null`. Deep decompilation (`analyze`, `decompile`, `function`, `search`, `xrefs`, `trace`) is therefore not possible on this host; this investigation uses the target-free native-macos + artifact-graph operations plus shipped files. **Observation**, not inference.

## Commands executed (and results)

| Command | Result |
|---|---|
| `capabilities` | table above |
| `inspect "/Applications/Alfred 5.app"` | **failed** `provider_unavailable` (`current_document` needs ghidra/hopper/ida; all rejected) — recorded verbatim |
| `inspect-artifact "<app>"` | ok — bundle inventory + root manifest (`raw/artifact.json`) |
| `inspect-macho Contents/MacOS/Alfred` | ok (`raw/macho.json`, evidence `ev_e882d72d01af…`) |
| `inspect-macho …/Alfred Framework.framework/Versions/A/Alfred Framework` | ok (`raw/macho_fw.json`) |
| `inspect-macho …Alfred Preferences.app/Contents/MacOS/Alfred Preferences` | ok (`raw/macho_prefs.json`) |
| `inspect-plist "<app>"` | ok (`raw/plist.json`) |
| `inspect-signature "<app>"` | ok — signed, id `com.runningwithcrayons.Alfred`, cdhash `f847e484…`, authority "TNT", team id null, hardened runtime false, entitlements {} |
| `inspect-native-dispatch-metadata` (three binaries, `--maxRecords 20000`) | ok (`raw/dispatch_exe.json`, `raw/dispatch_prefs.json`; framework result 11 MB kept in scratch→summarized into `raw/fw_classes.txt`, `raw/key_classes.txt`, `raw/key_selectors.txt`, `raw/exe_selectors.txt`, `raw/prefs_classes.txt`) |
| `decode-interface-builder "<app>"` | ok, bounded to 64 of 131 framework archives (`raw/main_nib_digest.txt`; limitation list names each omitted archive) |
| `decode-interface-builder "<app>/Contents/Preferences/Alfred Preferences.app"` | ok, 18/18 archives (`raw/nib_prefs.json`, `raw/prefs_nib_digest.txt`, per-document trees `raw/t_*.txt`) |
| `inspect-asset-catalog "<app>"` | ok (`raw/assets.json`) — 2 catalogs (identical sha in main bundle and prefs helper), 4 records total, incl. rendition "Accent Color" (P3) |
| `project-apple-application-graph "<app>"` | **failed** `invalid_request` (expects an Evidence JSON file, not a directory) — not retried |

Runtime UI ops: `observe-native-ui "/Applications/Alfred 5.app" --pid <pid> --windowId …` — see below.

## Distinguishing observed / inferred / unknown (REA rules)

- **Observed**: facts returned by the tools above or read directly from shipped files (binary headers, plist values, nib keyed-archive fields, table schemas read from SQLite, user data files). Each claim in docs 02–04 is tagged.
- **Derived**: computed from observed data by this session (class name sets from dispatch tables; nib containment trees from `contains` edges; string inventories via `strings -a` on shipped binaries).
- **Inferred**: plausible mechanisms not directly observed (e.g. "hotkeys use a Carbon-style event registration because classes expose `hotKeyRef`/`hotKeyID` and event-tap names"). Explicitly marked.
- **Unknown / not examined**: recorded per doc and consolidated in `gaps` of the task result.

## Known limitation of dispatch metadata (observed limitation)

The metadata reader decoded, for the framework binary, 279 non-metaclass classes / 4 953 method implementations while the binary contains ~780 distinct `Alfred*` name strings. The extra names appear in strings but had no decoded class/method records (nib class references, categories, or classes the reader did not decode). So: **class-name inventory is broad; method-map coverage is bounded** to the decoded records. All selector-level claims below come from decoded records.

## Runtime UI capture attempt (observed outcome)

Alfred was running (`pid 36982`). `observe-native-ui` requires an **existing** `windowId`; with the panel hidden the app owns no on-screen window, so any `windowId` is refused with `window-mismatch: Selected window is absent or belongs to another process` (verbatim). Attempts to reveal the panel — AppleScript `search` command, posting ⌥Space via CoreGraphics event APIs, `open` of the app to foreground — did not produce a capturable window in this session (the sdef `search` invocation blocked without displaying; the posted key event had no effect, indicating input-posting is not permitted for the driving process). **Runtime capture is treated as blocked by environment/permissions; static evidence stands and prior-session measurements are labelled as such where used.**

## Provenance conventions used in docs 02–04

Each claim cites one or more of:
- **`bin:<name>`** — shipped binary file + section/offset where applicable (`raw/macho*.json` carry exact UUID/offsets).
- **`disp:<class>`** — dispatch record for the class (class map + method list), source artifacts listed in the envelope.
- **`nib:<path>[:id]`** — decoded keyed-archive document (and object id) from `decode-interface_builder`.
- **`plist:<path>`, `db:<file>.<table>`, `fs:<path>`** — shipped plist / SQLite schema / on-disk file.
- **`strs:"<literal>"`** — literal string observed in a shipped binary (name/contract evidence, not behavior).
- **`runtime[prev]`** — measurement from an earlier capture in this investigation line (not re-measured this session).
