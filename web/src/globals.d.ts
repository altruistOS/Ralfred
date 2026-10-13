// Ambient types: theme + window hooks + CSS-as-text imports.
// (Imported by esbuild via loader:text; tsc sees the module declaration.)

interface RFTheme {
  BG?: string
  FIELD?: string
  TEXT?: string
  SUBTEXT?: string
  SELBG?: string
}

interface Window {
  __RF_THEME__?: RFTheme | null
  rfProbe?: (n: number) => void
  rfTheme?: () => void
  rfSearch?: (q: string) => void
  rfReset?: () => void
  rfApply?: (items: unknown) => void
  rfExec?: (n: number) => void
  rfLargeType?: (text: string, sizePx?: number) => void
  rfLargeTypeOff?: () => void
  __rfApplyLargeType?: (text: string | null) => void
  RF_DIAG?: number
}

declare module '*.css' {
  const content: string
  export default content
}
