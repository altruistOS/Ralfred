// Bridge to the Rust host. Pages are served from ralfred://api/index.html so
// every path here is a same-origin relative URL (ralfred://api/<path>).
//
// Rules learned the hard way (see src/engine.rs legacy page notes):
//  * Read bodies via text() + JSON.parse — response.json() can stall on this
//    WebView; avoid async/await so the bundle stays a flat promise chain.
//  * Duplicate *in-flight fetches to the same URL* get cancelled by WebKit and
//    the response is silently dropped, so every endpoint dedupes/coalesces.

import type { SearchResult, SettingsDoc, SettingValue } from './state'

function getJSON(path: string): Promise<unknown> {
  return fetch(path, { cache: 'no-store' }).then((r) =>
    r.text().then((t) => {
      if (!r.ok) throw new Error(t || 'HTTP ' + r.status)
      return JSON.parse(t)
    }),
  )
}

function getQuiet(path: string): Promise<void> {
  return fetch(path, { cache: 'no-store' })
    .then(() => undefined)
    .catch(() => undefined)
}

// ---- search -------------------------------------------------------------

export function search(q: string): Promise<SearchResult[]> {
  return getJSON('/search?q=' + encodeURIComponent(q)) as Promise<SearchResult[]>
}

// ---- exec / quicklook / hide ---------------------------------------------

export function exec(id: number): Promise<void> {
  return getQuiet('/exec?id=' + id)
}

export function ql(id: number): Promise<void> {
  return getQuiet('/ql?id=' + id)
}

export function hide(): Promise<void> {
  return getQuiet('/hide')
}

// ---- resize ---------------------------------------------------------------
// One in-flight request, last-write-wins: an update arriving while another is
// pending is coalesced and sent as soon as the in-flight one settles.

let resizeInFlight = false
let resizePending: number | null = null
let resizeLast = -1

export function resize(h: number): void {
  const n = Math.round(h)
  if (n === resizeLast && resizeInFlight) resizePending = null
  if (n === resizeLast && !resizeInFlight) return
  resizeLast = n
  if (resizeInFlight) {
    resizePending = n
    return
  }
  resizeInFlight = true
  getQuiet('/resize?h=' + n).then(() => {
    resizeInFlight = false
    if (resizePending != null) {
      const next = resizePending
      resizePending = null
      resize(next)
    }
  })
}

// ---- probe (selftest telemetry) -------------------------------------------

let probeInFlight = false
let probePendingRows: number | null = null
let probeLastRows = -1

export function probeRows(n: number): void {
  if (!probeInFlight) {
    if (n === probeLastRows) return
    probeLastRows = n
    probeInFlight = true
    fetch('/probe?rows=' + n, { cache: 'no-store' })
      .then(() => undefined)
      .catch(() => undefined)
      .then(() => {
        probeInFlight = false
        if (probePendingRows != null && probePendingRows !== probeLastRows) {
          const next = probePendingRows
          probePendingRows = null
          probeRows(next)
        }
      })
  } else {
    probePendingRows = n
  }
}

export function probeBG(bg: string): Promise<void> {
  return getQuiet('/probe?bg=' + bg)
}

// ---- settings -------------------------------------------------------------
// PATCH is serialized: only one in flight, and the freshest body wins if more
// edits land while a patch is running.

let patchChain: Promise<unknown> = Promise.resolve()
let patchPending: Record<string, SettingValue> | null = null

export function settingsGet(): Promise<SettingsDoc> {
  return getJSON('/settings') as Promise<SettingsDoc>
}

export function settingsPatch(values: Record<string, SettingValue>): Promise<unknown> {
  // merge so edits to *different* keys racing an in-flight patch are not lost
  patchPending = { ...(patchPending || {}), ...values }
  patchChain = patchChain.then(() => {
    const body = patchPending
    patchPending = null
    if (body === null) return undefined
    return fetch('/settings', {
      method: 'PATCH',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    }).then((r) =>
      r.text().then((t) => {
        if (!r.ok) {
          let msg = 'save failed'
          try {
            const j = JSON.parse(t)
            if (j && j.error) msg = String(j.error)
          } catch (e) {
            /* keep default message */
          }
          throw new Error(msg)
        }
      }),
    )
  })
  return patchChain
}
