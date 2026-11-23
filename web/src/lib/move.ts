/*
 * Moving an Entry: where a drop in the sidebar puts it, the places the Move
 * dialog offers, and moving it with a toast that offers to move it back.
 */
import { toast } from 'svelte-sonner'
import { call } from './ops'
import type { Area, Entry, MovedEntry, PathEntry, Place } from './operations.gen'
import { above, findEntry } from './tree'
import { UNDO_DURATION } from './trash'

/** Where on a row something is dropped: on its upper or lower edge, or onto the row itself. */
export type DropZone = 'before' | 'after' | 'inside'

/** A place within one Area: beneath an Entry or at its top (no `parent_id`), before a sibling or last (no `before_id`). */
export type PlaceInArea = Pick<Place, 'parent_id' | 'before_id'>

/**
 * Where dropping the Entry `draggedId` on the row of `targetId` puts it in
 * the same tree, or `null` when the drop moves nothing: onto itself or
 * beneath itself, or back where it is.
 */
export function dropPlace(entries: Entry[], draggedId: string, targetId: string, zone: DropZone): PlaceInArea | null {
  const dragged = findEntry(entries, draggedId)
  if (!dragged || draggedId === targetId || findEntry(dragged.children, targetId)) return null
  const place =
    zone === 'inside'
      ? { parent_id: targetId, before_id: undefined }
      : {
          parent_id: parentOf(entries, targetId),
          before_id: zone === 'before' ? targetId : nextSibling(entries, targetId, draggedId),
        }
  const now = { parent_id: parentOf(entries, draggedId), before_id: nextSibling(entries, draggedId, draggedId) }
  return place.parent_id === now.parent_id && place.before_id === now.before_id ? null : place
}

function parentOf(entries: Entry[], id: string): string | undefined {
  return above(entries, id).at(-1)
}

/** The sibling after an Entry, passing over `skipping`; `undefined` when it is the last. */
function nextSibling(entries: Entry[], id: string, skipping: string): string | undefined {
  const parent = parentOf(entries, id)
  const siblings = (parent === undefined ? entries : (findEntry(entries, parent)?.children ?? [])).filter(
    sibling => sibling.id === id || sibling.id !== skipping,
  )
  return siblings[siblings.indexOf(siblings.find(sibling => sibling.id === id)!) + 1]?.id
}

/** An Entry the Move dialog offers to move something beneath, indented by how deep it sits. */
export type Destination = { entry: Entry; depth: number }

/** Every Entry of a tree an Entry can be moved beneath, top to bottom: all but itself and what is beneath it. */
export function placesBeneath(entries: Entry[], movingId: string, depth = 0): Destination[] {
  return entries.flatMap(entry =>
    entry.id === movingId ? [] : [{ entry, depth }, ...placesBeneath(entry.children, movingId, depth + 1)],
  )
}

/** The Areas an Entry can be moved to: those where the person holds Write. */
export function writableAreas(areas: Area[]): Area[] {
  return areas.filter(area => area.role !== 'read')
}

/** The place an Entry was moved to, in words: its Area and what it is beneath. */
export function newPlace(moved: { area: Area; path: PathEntry[] }): string {
  if (moved.path.length === 0) return `the top of ${moved.area.name}`
  return [moved.area.name, ...moved.path.map(step => step.title)].join(' / ')
}

/**
 * Moves an Entry with everything beneath it, then names its new place in a
 * toast whose Undo moves it back: before the sibling it was before, or last
 * beneath what it was beneath once that sibling has gone. `changed` runs
 * after either, with the Areas whose trees changed.
 */
export async function moveEntry(
  entry: Pick<Entry, 'id' | 'title'>,
  to: Place,
  changed: (areaIds: string[]) => void | Promise<void>,
) {
  const moved: MovedEntry = await call('move_entry', { id: entry.id, ...to })
  const areas = [...new Set([moved.was.area_id, moved.area.id])]
  await changed(areas)
  toast(`Moved ${entry.title} to ${newPlace(moved)}`, {
    duration: UNDO_DURATION,
    action: {
      label: 'Undo',
      onClick: async () => {
        try {
          await call('move_entry', { id: entry.id, ...moved.was }).catch(() =>
            call('move_entry', { id: entry.id, ...moved.was, before_id: null }),
          )
          await changed(areas)
        } catch {
          toast.error(`${entry.title} could not be moved back. Move it from its menu.`)
        }
      },
    },
  })
}
