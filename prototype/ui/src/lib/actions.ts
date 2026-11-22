/*
 * What the Entry menu does, shared by the tree and the Move dialog. Each
 * action is one operation; the toast that follows says what happened and,
 * where it can be taken back, offers Undo (one more operation).
 */
import { toast } from 'svelte-sonner'
import { get_area, get_page, get_tree, move_entry, restore_entry, trash_entry } from './mock/ops.svelte'
import type { Entry, TreeEntry } from './mock/types'
import { router } from './router.svelte'
import { areaAddress, pageAddress } from './routes'
import { collapsed, ui } from './ui.svelte'

export async function copyLink(entry: Entry) {
  const address = location.origin + pageAddress(entry.id, entry.title)
  try {
    await navigator.clipboard.writeText(address)
    toast('Link copied', { description: entry.title })
  } catch {
    toast('The link could not be copied', { description: address })
  }
}

export function trash(entry: Entry, currentPageId: string | null) {
  trash_entry({ entry_id: entry.id })
  // The open Page went with it: show its Area instead.
  if (currentPageId && !get_page({ page_id: currentPageId })) {
    const area = get_area({ area_id: entry.area_id })!
    router.navigate(areaAddress(area.id, area.name))
  }
  toast('Moved to the Trash', {
    description: entry.title,
    action: { label: 'Undo', onClick: () => restore_entry({ entry_id: entry.id }) },
  })
}

/** Where an Entry can go: an Area, or a Folder or Page inside one. */
export type Destination = { area_id: string; parent_id: string | null }

export function moveTo(entry: Entry, to: Destination) {
  const from = { area_id: entry.area_id, parent_id: entry.parent_id, index: entry.position }
  move_entry({ entry_id: entry.id, area_id: to.area_id, parent_id: to.parent_id, index: Number.MAX_SAFE_INTEGER })
  if (to.parent_id) collapsed.delete(to.parent_id)
  ui.settled = entry.id
  setTimeout(() => ui.settled === entry.id && (ui.settled = null), 900)
  toast('Moved to ' + describe(to), {
    description: entry.title,
    action: { label: 'Undo', onClick: () => move_entry({ entry_id: entry.id, ...from }) },
  })
}

/** "Platform / Runbooks": the Area and the Entries down to the destination. */
export function describe(to: Destination): string {
  const area = get_area({ area_id: to.area_id })!
  if (!to.parent_id) return area.name
  const path: string[] = []
  const find = (list: TreeEntry[], trail: string[]): boolean =>
    list.some((e) => (e.id === to.parent_id ? (path.push(...trail, e.title), true) : find(e.children, [...trail, e.title])))
  find(get_tree({ area_id: area.id }), [])
  return [area.name, ...path].join(' / ')
}
