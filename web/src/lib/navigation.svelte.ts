import { SvelteMap } from 'svelte/reactivity'
import { call, OperationError } from './ops'
import type { Area, Entry, Tree } from './operations.gen'
import { reportFailure } from './toasts'

let areas = $state.raw<Area[] | null>(null)
let areasFailed = $state(false)
const trees = new SvelteMap<string, Entry[]>()
/** How many deleted Subtrees each loaded Area's Trash holds; `null` for someone who cannot see it. */
const trashSizes = new SvelteMap<string, number | null>()
/** The Areas whose tree is being fetched for the first time. */
const fetching = new Set<string>()
/** Counts the people this has been shown to, so an answer meant for the one before is dropped. */
let shownTo = 0

/**
 * What the sidebar shows: the Areas the person can read and the trees of the
 * ones that have been opened. The screens read the same trees, so a change
 * made on one side shows on the other.
 */
export const navigation = {
  /** `null` until the list has arrived. */
  get areas() {
    return areas
  },
  /** True when the list could not be loaded the last time it was asked for. */
  get areasFailed() {
    return areasFailed
  },
  /** The top of an Area's tree, in order; `undefined` until it has been loaded. */
  tree(areaId: string): Entry[] | undefined {
    return trees.get(areaId)
  },
  /** How many deleted Subtrees the Trash of an Area holds, once its tree is loaded; `null` when the person cannot see that Trash. */
  trashSize(areaId: string): number | null | undefined {
    return trashSizes.get(areaId)
  },
  /** Loads the list of Areas again, after one was created or renamed or a Role changed. A failure is said in a toast. */
  async refreshAreas() {
    const asked = shownTo
    try {
      const list = await call('list_areas', {})
      if (asked !== shownTo) return
      areas = list.areas
      areasFailed = false
    } catch (caught) {
      if (asked !== shownTo) return
      areasFailed = true
      reportFailure(caught, 'The Areas could not be loaded. Reload the page to try again.')
    }
  },
  /** Loads the tree of an Area, again if it is known already. Fails as the operation does. */
  async loadTree(areaId: string): Promise<Tree> {
    const asked = shownTo
    fetching.add(areaId)
    try {
      const tree = await call('get_tree', { area_id: areaId })
      if (asked === shownTo) {
        trees.set(areaId, tree.entries)
        trashSizes.set(areaId, tree.trash ?? null)
      }
      return tree
    } catch (caught) {
      // An Area that can no longer be read keeps nothing on screen, and leaves the list.
      if (asked === shownTo && caught instanceof OperationError && caught.kind === 'not_found') {
        trees.delete(areaId)
        trashSizes.delete(areaId)
        void this.refreshAreas()
      }
      throw caught
    } finally {
      if (asked === shownTo) fetching.delete(areaId)
    }
  },
  /** Loads the tree of an Area unless it is here or on its way. A failure is said in a toast. */
  async showTree(areaId: string) {
    if (trees.has(areaId) || fetching.has(areaId)) return
    const asked = shownTo
    try {
      await this.loadTree(areaId)
    } catch (caught) {
      if (asked === shownTo) reportFailure(caught, 'The tree of this Area could not be loaded. Reload the page to try again.')
    }
  },
  /** Drops everything, so the next person to sign in starts from nothing. */
  forget() {
    shownTo += 1
    fetching.clear()
    areas = null
    areasFailed = false
    trees.clear()
    trashSizes.clear()
  },
}
