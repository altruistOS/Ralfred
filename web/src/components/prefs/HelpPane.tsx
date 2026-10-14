import type { PaneProps } from './common'
import { toStr } from './common'

function Cmd({ children }: { children: string }) {
  return <code className="pf-kbd">{children}</code>
}

function HRow({ cmd, desc }: { cmd: string; desc: string }) {
  return (
    <div className="pf-hrow">
      <span className="pf-hcmd">
        <Cmd>{cmd}</Cmd>
      </span>
      <span className="pf-hdesc">{desc}</span>
    </div>
  )
}

/**
 * Help pane: static usage reference — query syntax, prefixes and shortcuts.
 * No network calls; the hotkey row echoes the live value from settings.
 */
export function HelpPane({ values }: PaneProps) {
  const hotkey = toStr(values.hotkey).toUpperCase() || 'the configured hotkey'
  return (
    <div className="pf-pane pf-help" id="pf-help" tabIndex={-1}>
      <div className="pf-help-sec">
        <div className="pf-title">Getting started</div>
        <div className="pf-hrow">
          <span className="pf-hcmd">
            <kbd className="pf-kbd">{hotkey}</kbd>
          </span>
          <span className="pf-hdesc">Summon the search panel, start typing anywhere.</span>
        </div>
      </div>

      <div className="pf-help-sec">
        <div className="pf-title">Query syntax</div>
        <HRow cmd="calc 2+2" desc="Calculator — evaluate arithmetic expressions." />
        <HRow cmd="gt github search" desc="GitHub search for the given query." />
        <HRow cmd="c name" desc="Look up a name (contacts-style lookup)." />
        <HRow cmd="op" desc="Open — list available actions / open something." />
        <HRow cmd="bm" desc="Bookmarks — jump to a saved bookmark." />
        <HRow cmd="music" desc="Music — control or search your library." />
        <HRow cmd="lt" desc="Large type — show the selection enlarged." />
        <HRow cmd="dict" desc="Dictionary lookup for the given word." />
        <HRow cmd=", (comma)" desc="Clipboard history — pick a previous copy." />
        <HRow cmd="/ (slash)" desc="File search — type a path to browse files." />
        <HRow cmd="s " desc="Snippets — search your saved snippets." />
      </div>

      <div className="pf-help-sec">
        <div className="pf-title">Shortcuts in the panel</div>
        <HRow cmd="Enter" desc="Open the selected result." />
        <HRow cmd="Tab" desc="Cycle the actions available for the selection." />
        <HRow cmd="Cmd+Y" desc="Quick Look the selected result." />
        <HRow cmd="Cmd+L" desc="Large type overlay for the selected result." />
        <HRow cmd="Cmd+," desc="Open this preferences window." />
      </div>
    </div>
  )
}
