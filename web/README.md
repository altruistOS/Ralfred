# ralfred web

The search-panel front end for the `ralfred` macOS launcher. React 18 +
TypeScript, bundled by **esbuild** (no vite, no webpack) into exactly two
files in `dist/`: `index.html` and `bundle.js`. The Rust host embeds both at
compile time and serves them from its custom scheme — everything below is
pinned by that embedding contract.

## Build / check / smoke

```sh
npm install        # react, react-dom, esbuild, typescript
npm run build      # esbuild src/index.tsx -> dist/bundle.js (minified)
npm run check      # tsc --noEmit
npm run smoke      # node scripts/smoke.mjs — pure reducer/debounce tests
npm run dev        # build + watch
```

`npm run build` emits CSS *inside* `bundle.js` (esbuild's `--loader:.css=text`
import + runtime `<style>` injection in `src/index.tsx`), so no extra runtime
assets exist in `dist/` beyond `index.html` + `bundle.js`.

## Embedding contract (Rust side)

- `src/webassets.rs` does `include_str!("../web/dist/index.html")` +
  `include_bytes!("../web/dist/bundle.js")` and serves them under the
  `ralfred://api/` scheme (`ralfred://api/index.html`, `ralfred://api/bundle.js`).
- `index.html` must keep the literal token `__THEME__` and the string
  `bundle.js` — there are Rust tests enforcing this. The host replaces
  `__THEME__` with a JSON theme object `{"BG":...,"FIELD":...,"TEXT":...,
  "SUBTEXT":...,"SELBG":...}` whose values are lowercase hex **without `#`**.
- `has_real_assets()` treats a bundle >4 KB as real; keep the build minified.
- Relative paths only: the page is loaded from `ralfred://api/index.html`, and
  `./bundle.js` plus root-relative fetches (`/search?q=`, `/settings`, …) all
  resolve against `ralfred://api/*`. Never use absolute `http(s)` paths.

## Bridge endpoints (all same-origin, GET unless noted)

| Path | Purpose |
|---|---|
| `/search?q=<urlencoded>` | `[{t,s,k,i}, …]` — title, subtitle, kind, icon URL |
| `/exec?id=<n>` | activate row n |
| `/ql?id=<n>` | quick-look row n |
| `/settings` | `GET` doc `{schema:{sections:[{title,fields:[{key,label,type,options?}]}]},values:{...}}`, `PATCH` flat values object |
| `/hide` | panel dismiss |
| `/resize?h=<px>` | host resizes the window (clamps 71..972) |
| `/probe?rows=<n>` / `/probe?bg=<hex>` | selftest telemetry |

Notes: bodies are read via `text()` + `JSON.parse` (never `response.json()`),
because WebView body reads with the native parser can stall; and duplicate
*in-flight* fetches to one URL are cancelled silently by WebKit, so search,
resize and probe all coalesce/dedupe (last-write-wins, one in flight).

## Window hooks driven by the host (`evaluate_script`)

- `rfSearch(q)` — set query state + fetch immediately (selftest entry point)
- `rfReset()` — clear everything, refocus input (panel summon)
- `rfProbe(n)` — reports visible row count (selftest's DOM_ROWS)
- `rfTheme()` — reports `/probe?bg=` of the active BG (selftest theme check)
- `rfLargeType(text, px)` / `rfLargeTypeOff()` — fullscreen large type
- `rfExec(n)` / `rfApply(itemsJsonArray)` — exec row / direct items route
- `__rfApplyLargeType(text)` — React-side hook alias for large type

Keyboard: ↑/↓ navigate, ⏎ exec, ⌘1–9 exec row n−1 (hint column shows ⏎ on row
0, ⌘2…⌘9 on rows 1–9), ⌘Y quick-look, ⌘, settings, Esc closes overlays or
calls `/hide`. Rows render 46px tall with 28px icons and carry the legacy `i`
class (`querySelectorAll('.i')` compatibility) plus `role=listbox/option`
semantics. The panel is empty (field only) on an empty query; a
ResizeObserver keeps the host window height in sync via `/resize?h=`.
