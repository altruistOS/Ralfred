// Entry point for scripts/smoke.mjs: re-export the pure pieces of the app
// (no DOM, no React) so a plain `node` process can exercise them.
export { reducer, initialState, debounce } from './state'
