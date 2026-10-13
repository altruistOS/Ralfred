// Pure state: reducer + debounce helper. No DOM/React imports, so this is
// unit-testable under plain `node` (see scripts/smoke.mjs).

export interface SearchResult {
  /** title */ t: string
  /** subtitle */ s: string
  /** kind */ k: string
  /** icon URL (may be empty) */ i: string
}

export type SettingValue =
  | string
  | number
  | boolean
  | string[]
  | Record<string, string>

export interface SettingsField {
  key: string
  label: string
  type: string
  hint?: string
  options?: string[]
  restart?: boolean
  min?: number
  max?: number
}

export interface SettingsSection {
  id?: string
  title: string
  fields: SettingsField[]
}

export interface SettingsDoc {
  schema: { sections: SettingsSection[] }
  values: Record<string, SettingValue>
}

export interface AppState {
  query: string
  items: SearchResult[]
  sel: number
  settingsOpen: boolean
  settingsDoc: SettingsDoc | null
  toast: string | null
  largeType: string | null
  largeTypeSize: number
}

export type Action =
  | { type: 'QUERY'; query: string }
  | { type: 'ITEMS'; items: SearchResult[] }
  | { type: 'SEL'; index: number }
  | { type: 'MOVE'; delta: number }
  | { type: 'SETTINGS_OPEN'; doc: SettingsDoc }
  | { type: 'SETTINGS_CLOSE' }
  | { type: 'SETTINGS_SET'; key: string; value: SettingValue }
  | { type: 'TOAST'; text: string | null }
  | { type: 'LARGE'; text: string | null; size?: number }
  | { type: 'CLEAR' }

export const initialState: AppState = {
  query: '',
  items: [],
  sel: 0,
  settingsOpen: false,
  settingsDoc: null,
  toast: null,
  largeType: null,
  largeTypeSize: 80,
}

export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case 'QUERY':
      return state.query === action.query ? state : { ...state, query: action.query }

    case 'ITEMS': {
      const items = Array.isArray(action.items) ? action.items : []
      const sel = Math.max(0, Math.min(state.sel, items.length - 1))
      return { ...state, items, sel }
    }

    case 'SEL': {
      const n = state.items.length
      if (n === 0) return state
      const sel = Math.max(0, Math.min(action.index, n - 1))
      return sel === state.sel ? state : { ...state, sel }
    }

    case 'MOVE': {
      const n = state.items.length
      if (n === 0) return state
      const sel = ((state.sel + action.delta) % n + n) % n // wrap like a picker wheel
      return { ...state, sel }
    }

    case 'SETTINGS_OPEN':
      return { ...state, settingsOpen: true, settingsDoc: action.doc }

    case 'SETTINGS_CLOSE':
      return { ...state, settingsOpen: false }

    case 'SETTINGS_SET': {
      if (!state.settingsDoc) return state
      const values = { ...state.settingsDoc.values, [action.key]: action.value }
      if (state.settingsDoc.values[action.key] === action.value) return state
      return { ...state, settingsDoc: { ...state.settingsDoc, values } }
    }

    case 'TOAST':
      return { ...state, toast: action.text }

    case 'LARGE':
      return {
        ...state,
        largeType: action.text,
        largeTypeSize: action.size != null ? action.size : state.largeTypeSize,
      }

    case 'CLEAR':
      return {
        ...state,
        query: '',
        items: [],
        sel: 0,
        settingsOpen: false,
        toast: null,
        largeType: null,
      }

    default:
      return state
  }
}

/** Leading-edge-free trailing debounce; last call wins. Pure timing only. */
export function debounce<A extends readonly unknown[]>(
  fn: (...args: A) => void,
  ms: number,
): ((...args: A) => void) & { cancel(): void } {
  let timer: ReturnType<typeof setTimeout> | undefined
  const wrapped = (...args: A): void => {
    if (timer !== undefined) clearTimeout(timer)
    timer = setTimeout(() => {
      timer = undefined
      fn(...args)
    }, ms)
  }
  wrapped.cancel = (): void => {
    if (timer !== undefined) {
      clearTimeout(timer)
      timer = undefined
    }
  }
  return wrapped
}
