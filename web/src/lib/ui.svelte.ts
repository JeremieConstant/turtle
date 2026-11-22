/*
 * View state that is not data: which Area is open in the sidebar, which
 * Entries are folded, and whether the drawer is out. It changes nothing in
 * the Workspace, so it needs no operation, and it lives outside the screens,
 * so it survives moving between them.
 */
import { SvelteSet } from 'svelte/reactivity'

export const ui = $state({
  /** The Area whose tree the sidebar shows. */
  openArea: null as string | null,
  /** The sidebar as a drawer over the screen, on a narrow window. */
  drawerOpen: false,
})

/** The Entries whose children are folded away. Everything is unfolded until someone folds it. */
export const collapsed = new SvelteSet<string>()
