// Theme handling. The Rust host rewrites index.html replacing the `__THEME__`
// token with {"BG":"1e1e1e",...} (hex, no leading '#'), so window.__RF_THEME__
// is populated before bundle.js runs. Defaults keep a browser-opened page sane.

const DEFAULTS: Record<string, string> = {
  BG: '1e1e1e',
  FIELD: '111010',
  TEXT: 'f2f2f2',
  SUBTEXT: '8e8e93',
  SELBG: '3a3a3c',
  ACCENT: '3297c1',
}

const VARS: Array<[string, keyof RFTheme]> = [
  ['--rf-bg', 'BG'],
  ['--rf-field', 'FIELD'],
  ['--rf-text', 'TEXT'],
  ['--rf-subtext', 'SUBTEXT'],
  ['--rf-selbg', 'SELBG'],
  ['--rf-accent', 'ACCENT'],
]

function hexColor(raw: string | undefined, def: string): string {
  const s = (raw == null ? '' : String(raw)).trim().replace(/^#+/, '')
  return '#' + (s.length > 0 ? s : def)
}

export function applyTheme(): void {
  const t: RFTheme = (typeof window !== 'undefined' && window.__RF_THEME__) || {}
  for (const [cssVar, key] of VARS) {
    document.documentElement.style.setProperty(cssVar, hexColor(t[key], DEFAULTS[key]))
  }
}

/** Current background as lowercase hex without '#', for /probe?bg=. */
export function currentBG(): string {
  const t: RFTheme = (typeof window !== 'undefined' && window.__RF_THEME__) || {}
  const raw = t.BG != null ? String(t.BG) : DEFAULTS.BG
  return raw.trim().toLowerCase().replace(/^#+/, '')
}
