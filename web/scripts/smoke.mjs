// Smoke test: bundle the pure state layer with esbuild's JS API and assert
// reducer + debounce behaviour. No DOM, no React, no vitest. Exits 0 on pass.

import { buildSync } from 'esbuild'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '..')

const outDir = mkdtempSync(resolve(tmpdir(), 'ralfred-smoke-'))
const outFile = resolve(outDir, 'state.mjs')
try {
  buildSync({
    entryPoints: [resolve(root, 'src/smoke-entry.ts')],
    bundle: true,
    format: 'esm',
    outfile: outFile,
    logLevel: 'silent',
  })

  const { reducer, initialState, debounce } = await import(pathToFileURL(outFile).href)
  const mk = (n) => Array.from({ length: n }, (_, i) => ({ t: `T${i}`, s: `S${i}`, k: 'app', i: '' }))

  // -- reducer transitions --
  const s0 = initialState
  assert.equal(s0.query, '')
  assert.equal(s0.sel, 0)
  assert.equal(s0.settingsOpen, false)
  assert.equal(s0.largeType, null)

  const s1 = reducer(s0, { type: 'QUERY', query: 'hi' })
  assert.equal(s1.query, 'hi')
  assert.equal(reducer(s1, { type: 'QUERY', query: 'hi' }), s1) // no-op returns same ref

  const s2 = reducer(s1, { type: 'ITEMS', items: mk(12) })
  assert.equal(s2.items.length, 12)
  assert.equal(s2.sel, 0)

  const s3 = reducer(s2, { type: 'SEL', index: 5 })
  assert.equal(s3.sel, 5)
  assert.equal(reducer(s2, { type: 'SEL', index: 99 }).sel, 11) // clamps
  assert.equal(reducer(s2, { type: 'SEL', index: -3 }).sel, 0)

  const s4 = reducer(s3, { type: 'MOVE', delta: 1 })
  assert.equal(s4.sel, 6)
  assert.equal(reducer(s2, { type: 'MOVE', delta: -1 }).sel, 11) // wraps up
  assert.equal(reducer(s3, { type: 'MOVE', delta: 1 }).sel, 6)
  assert.equal(reducer(s2, { type: 'MOVE', delta: 12 }).sel, 0) // full wrap
  assert.equal(reducer(initialState, { type: 'MOVE', delta: 1 }), initialState) // no items -> no move

  const s5 = reducer(reducer(s2, { type: 'SEL', index: 11 }), { type: 'MOVE', delta: 1 })
  assert.equal(s5.sel, 0) // wrap at bottom

  const doc = {
    schema: { sections: [{ title: 'General', fields: [{ key: 'a', label: 'A', type: 'string' }] }] },
    values: { a: '1', n: 9, on: true },
  }
  const s6 = reducer(s5, { type: 'SETTINGS_OPEN', doc })
  assert.equal(s6.settingsOpen, true)
  assert.equal(s6.settingsDoc, doc)

  const s7 = reducer(s6, { type: 'SETTINGS_SET', key: 'a', value: '2' })
  assert.equal(s7.settingsDoc.values.a, '2')
  assert.equal(s6.settingsDoc.values.a, '1') // immutable update
  assert.equal(reducer(s7, { type: 'SETTINGS_SET', key: 'a', value: '2' }), s7)

  const s8 = reducer(s7, { type: 'SETTINGS_CLOSE' })
  assert.equal(s8.settingsOpen, false)
  assert.notEqual(s8.settingsDoc, null) // doc cached for reopen

  const s9 = reducer(s8, { type: 'LARGE', text: 'Hello', size: 64 })
  assert.equal(s9.largeType, 'Hello')
  assert.equal(s9.largeTypeSize, 64)
  assert.equal(reducer(s9, { type: 'LARGE', text: null }).largeType, null)

  const sA = reducer(s8, { type: 'TOAST', text: 'err' })
  assert.equal(sA.toast, 'err')
  assert.equal(reducer(sA, { type: 'TOAST', text: null }).toast, null)

  // CLEAR keeps the cached settings doc, wipes the rest
  const sB = reducer(s9, { type: 'CLEAR' })
  assert.equal(sB.query, '')
  assert.equal(sB.items.length, 0)
  assert.equal(sB.sel, 0)
  assert.equal(sB.largeType, null)
  assert.notEqual(sB.settingsDoc, null)

  const unknown = reducer(s0, { type: 'NOPE' })
  assert.equal(unknown, s0)

  // -- debounce: last write wins, one trailing call --
  const seen = []
  const d = debounce((v) => seen.push(v), 15)
  d('a')
  d('b')
  d('c')
  assert.deepEqual(seen, [], 'nothing fires before the timer')
  await new Promise((r) => setTimeout(r, 60))
  assert.deepEqual(seen, ['c'])

  d.cancel() // no further calls scheduled
  await new Promise((r) => setTimeout(r, 30))
  assert.deepEqual(seen, ['c'])

  console.log('smoke: ok')
} finally {
  rmSync(outDir, { recursive: true, force: true })
}
