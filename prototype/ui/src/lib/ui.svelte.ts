/*
 * View state that is not data: which Area is open in the navigation and which
 * Entries are collapsed. It changes nothing in the Workspace, so it needs no
 * operation.
 */
import { SvelteSet } from 'svelte/reactivity'

export const ui = $state({
  /** The Area whose tree the navigation shows. */
  openArea: null as string | null,
  /** The Entry that just landed after a drag, so it can settle visibly. */
  settled: null as string | null,
  /** The navigation drawer on narrow screens. */
  drawerOpen: false,
  /** A Page whose editor should take the focus once it is on screen ("Edit"). */
  focusEditor: null as string | null,
})

/** Entries whose children are hidden. Everything is expanded by default. */
export const collapsed = new SvelteSet<string>()
