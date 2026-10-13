# 03 — UI Layout Specs (per surface)

Tag legend per `01-tooling-and-method.md`: **nib** = decoded Interface Builder geometry; **runtime[prev]** = measured in an earlier capture of this investigation line, *not re-measured this session* (static corroboration cited separately); **disp**/**strs** = binary evidence, geometry inferred from naming.

## 1. Main search panel

**Structure (observed)**: `NSPanel` subclass `AlfredWindow` (key-capable, background-movable, blur-configured) hosting one active view via `replaceView:`/`replaceView:stack:` (stacking + `destackCurrentView` for drill-ins such as actions). Views: `AlfredDefaultView` (input surface), `AlfredDefaultResultsView` (results, code-drawn — there is **no nib** for it), `AlfredBufferView`, `AlfredUpdateView`. (disp, exe classes.)

**Geometry — empty state (runtime[prev], corroborated by strs)**:
- Panel 718 × 71 px, corner radius 12.
- Field fill `#111010` over panel fill `#171516`; hint text `#a2a2a2`; selection tint `#2f767c`.
- Theme-adjustable via preference keys (`strs`): `search.background`, `search.backgroundSelected`, `search.text`, `search.text.color`, `search.text.colorSelected`, `search.text.font`, `search.text.size`, `search.paddingHorizontal`, `search.paddingVertical`, `search.spacing`, `search.roundness`, `search.text.markdown/plaintext sizing` (as `appearance.views.textsizing.*`).
- Position/visibility options: `appearance.options.xPoint|yPoint|saveposition|showOnScreen|focusmode|hidehat|hidemenu|hidesubtext|hideshortcuts|autohighlight|visibleresults|showscrollbar|scrollbarmode`, `appearance.screen`, `appearance.darkthemeuid|lightthemeuid` (per-light/dark theme selection; `strs`). The measured palette is *the* "native dark" preset; other themes ship as `.alfredappearance` JSON with the same key names (fs).
- Empty state shows only the input line + optional "hat" (left icon/menu affordance, `hidehat`) and optional menu (right, `hidemenu`).

**Geometry — results state (runtime[prev] + disp/nib corroboration)**:
- Results grow the panel downward via `resizeWindowAndCurrentView` (disp).
- Row pitch 46 pt, icon 28 pt — from prior runtime capture. Corroborated statically only by the row height convention of the preferences search-cell (40 pt in its nib, different surface — see §2.4) and by `AlfredDefaultResultsView setupFonts`/`updateResultsViewFrame` (disp). Treat row metrics as runtime[prev].
- Row anatomy (code-drawn; `disp:AlfredDefaultResultsView` + `strs` names `AlfredTableRowView/Cell`, `AlfredListCell`): [28 pt icon | title line + subtitle line][right-aligned modifier/hotkey hint area]. Titles/subtitles are attributed (placeholders: `replaceQueryPlaceholder:withText:`, `replaceVariablePlaceholdersIn:result:`). Modifier overrides: `AlfredDefaultResultModOverride` + result `mods` JSON (doc 04) drive alternate subtitle/arg on ⌘/⌥/fn/ctrl; `clearMaintainSelection`, `highlightNext/Previous` track it.
- Selection: `AlfredTableView usePurplyRows` flag plus `search.backgroundSelected` (strs) govern selected-row fill.

## 2. Preferences window (helper app)

**2.1 Window** (nib `MainMenu.nib`): 1180 × 700, titled/closable/miniaturizable/resizable; toolbar with search field 96×22 and status label. Left **tab bar 80 pt wide**, full height (`AlfredPreferencesTabBarView`): items added in code (`addItemWithID:label:image:contentClass:`), each item = icon + label, custom-drawn (`disp:AlfredPreferencesTabBarItem`). Content region 1100 × 700.

**2.2 Pane contract**: every pane nib roots in an `NSView` 1100 × 640 on `AlfredBackgroundView`, swapped into the content region (nib; all panes share 1100×640). Tab identity ↔ `AlfredPreferences*` controller (disp, prefs classes). Panes: General, Features, Appearance, Workflows, Advanced, (Remote, Update, Help, Powerpack, Usage per nib inventory doc 02 §3).

**2.3 Features pane** (nib `AlfredPreferencesFeatures.nib`): master list sidebar 250 pt (table 240 × 640, auto column); detail: 64×64 icon @ (263,566), title 22 pt line, description label under title, bottom bar 850 × 60 with help button @ (807,16, 25×25); config area `{250,0,850,580}` hosts the per-feature `AlfredFeature*Config` view loaded from its nib (doc 02 §3). Powerpack gate sheet 805 × 500 with image, paragraph, Buy/About buttons.

**2.4 Preferences search overlay** (nib `AlfredPreferencesSearchPanel.nib`): HUD `NSPanel` 360 × 470; table fills window; **row cell 340 × 40**: title textfield 290 × 18 at (40, 19), subtitle 290 × 17 at (40, 2) — the 40 pt icon gutter is consistent with the 46 pt main-panel row pitch being a different (larger) surface (consistency inference, not proof).

**2.5 Appearance pane** (nib): theme list table 240 × 618; controls column: blur radio set (Classic Blur / No Blur / Visual Effect Dark / Visual Effect Light), popups (screen position/visible results/scrollbar mode), checkboxes (e.g. auto-highlight, hide shortcuts…), "Options"/"Help" buttons 82×32; live theme preview view (`AlfredPreferencesAppearanceThemeView` + `Renderer` + `Hitzone`); position picker (`AlfredPreferencesAppearancePositionView`).

**2.6 Workflows pane** (nib `AlfredPreferencesWorkflows.nib`): `NSSplitView` 1100 × 640. Left column 253 pt: workflow search field 213 × 22 (y 608), buttons −/+/action at y −2 (25 × 26) + (30 × 30 gear @ (223,603)), list `AlfredTableView` 243 × 576 inside scroll view. Right 846 pt: `AlfredBackgroundView` canvas host (`AlfredWorkflowCanvasController`; the canvas itself is the helper's only WebKit use — disp import + inferred). Sheets: "User Defaults" (workflow config) 504 × 424, code/text sheet 600 × 400 (`AlfredTextView` 558 × 278), workflow metadata editor 583 × 365 (icon well 70 × 70, popups 364×26), large configuration sheet 880 × 680.

**2.7 Advanced pane** (nib): dense 150-control form; observed items: reindex section (popup 236×26, "Rebuild macOS Metadata" button 194×32, "Delete .Spotlight-V100…" 251×18 with Full-Disk-Access note), sync-folder chooser, per-Mac settings, shutdown/diagnostics toggles; reindex confirm sheet 480 × 474 (`AlfredPreferencesAdvancedReindex.nib`).

**2.8 Onboarding** (framework nibs): "Welcome to Alfred" window 740 × 550 with step panes (`AlfredOnboardingStep*` nibs) and a step-migration variant; permissions window 545 × 296.

## 3. Result-row anatomy (summary)

Observed fields: icon (28 pt square, runtime[prev]; `imageCacheUpdated:` async loading, disp), title + subtitle (variable/placeholder substitution, disp), optional right-side hint text (`hideshortcuts` toggle, strs), modifier overlay (mods, disp), per-result type semantics (file/contact/websearch/action) drive icon (`fileicon|filetype` icon types, doc 04) and action set (universal actions). Accessibility: results view implements `announceSelectedResultToAccessibility:` (disp).

## 4. Type / preview selectors (observed, from doc 02 §2.4)

`AlfredFeaturePreviews` (config pane nib `AlfredFeaturePreviews.nib`), preview machinery `AlfredPreview(Item)`, `acceptsPreviewPanelControl:/begin/end`, `previewPanel:handleEvent:`, `showPreviewForURL:delegate:`, `updatePreviewWithURL:`, `generateQuicklookURL`, `toggleQuickLook`; large type `AlfredLargeType(View)` + `appearance.largetype.*`. Preview surface geometry (QuickLook-style panel size, large-type overlay): **unknown** — no nib and no runtime measurement this session.

## 5. Keybinding list (observed where possible)

Menu shortcuts (nib `MainMenu.nib`): ⌘F Search Preferences, ⇧F Filter Workflows, standard ⌘Z/⌘X/⌘C/⌘V/⌘A, ⌘M minimize, ⌘W close, ⌘Q quit prefs, ⌃⌘F full screen.

Search panel (strs command set + nib text): `↑` with empty query = last 20 typed queries (nib text in `AlfredPreferencesAdvanced.nib`); ⌃↑ / ⌃↓ scroll query history (nib text, same source); ↑/↓ cycle results (`moveUp/moveDown` via `doCommandBySelector:`, disp); → drill into actions / file-navigation (pref `features.filesearch.actions.showWithRightArrow`, `features.filesearch.navigation.arrowsForFolderNavigation`); ← back (`alfred.navigation.copy` / altaction, strs); ⏎ open/action (`insertNewline`); ⇥/⌥→ universal-actions panel (`showWithTab`, `showActionsPanelForSelection`, disp); ⌫ delete-back (`deleteBackward`); esc hide (`cancelOperation` → `hideAlfred`, disp+strs); ⌥-actions (`alfred.navigation.altaction`, strs); Space/⇧Space quicklook (`toggleQuickLook`, `quicklook.showWithShift`); ⌘L large type (`text.largetype`, `features.1password`… — large-type via `AlfredLargeType`, strs).

Global/feature hotkeys (strs keys; defaults assigned at first run — **values unknown** without reading a user plist, this install's live prefs expose none):
- `features.clipboard.hotkey` (clipboard history viewer)
- `features.snippets.hotkey`
- `features.itunes.hotkey` (+ `features.1password` none, uses keyword)
- Workflow hotkey triggers: `alfred.workflow.trigger.hotkey` objects carry mod+key in each workflow's info.plist (doc 04)
- Remote server: `remote.serverRestartKeyword`, `remote.serverStopKeyword` (+ enabled flags)
- `features.filesearch.actions.finderSelectionHotkey`, `features.filesearch.navigation.previousPathHotkey`

**Inferred default**: ⌥Space as the classic show/hide hotkey — consistent with `AlfredDefaultHotkey` (disp, exe) but the actual key/mod is stored per-user and was not observed.

## 6. Per-feature config panes (framework nibs, decoded)

One pane per feature class loaded into Features detail area (nib names ↔ `AlfredFeature*Config` classes, doc 02 §2.2). Geometry of individual config controls (checkbox rows, file pickers, popup rows, slider rows, textarea rows) follows the `AlfredConfigurableConfig*` family: `ConfigurableConfigBackend(Item)`, `ConfigItemCheckbox|FilePicker|PopupButton|Slider|TextArea|TextField` (each with `Config` nib) — these are the *generic* preference-form mechanism reused by many feature panes (observed class + nib pairs). Exact per-control sizes: not enumerated here; see `raw/main_nib_digest.txt` for frames of decoded ones.
