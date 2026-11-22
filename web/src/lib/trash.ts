/*
 * The words the Trash shows for what is in it, and moving an Entry there with
 * a toast that offers to undo it.
 */
import { toast } from 'svelte-sonner'
import { call } from './ops'
import type { Area, Author, Entry, TrashEntry } from './operations.gen'
import { authorName } from './versions'

/** How long a toast that offers to undo a deletion stays, in milliseconds: longer than others, to leave time to reach it. */
export const UNDO_DURATION = 10_000

/** How many days are left before something is removed for good. */
export function daysLeft(days: number): string {
  return days <= 1 ? 'Last day' : `${days} days left`
}

/** Who deleted something; Turtle no longer names them once they are gone. */
export function deletedBy(author: Author | null | undefined): string {
  return `Deleted by ${author ? authorName(author) : 'someone no longer here'}`
}

/** Whether the days left are few enough to be marked. */
export function endsSoon(days: number): boolean {
  return days <= 3
}

/** Where a deleted Entry was, and where restoring puts it when that place is gone too. */
export function whereItWas(entry: Pick<TrashEntry, 'was_beneath' | 'restores_to_top'>, areaName: string): string {
  if (entry.was_beneath.length === 0) return `Was at the top of ${areaName}`
  const was = `Was in ${entry.was_beneath.join(' / ')}`
  return entry.restores_to_top ? `${was}, which is in the Trash too. Restores to the top of ${areaName}.` : was
}

/**
 * Moves an Entry with everything beneath it to the Trash, then says so in a
 * toast whose Undo restores it to its place. `changed` runs after either, to
 * show the tree as it is now.
 */
export async function moveToTrash(entry: Entry, area: Area, changed: () => void | Promise<void>) {
  await call('move_to_trash', { id: entry.id })
  await changed()
  toast(`Moved ${entry.title} to the Trash`, {
    description: entry.children.length > 0 ? 'With everything beneath it.' : undefined,
    duration: UNDO_DURATION,
    action: {
      label: 'Undo',
      onClick: async () => {
        try {
          await call('restore_from_trash', { id: entry.id })
          await changed()
        } catch {
          toast.error(`${entry.title} could not be restored here. Restore it from the Trash of ${area.name}.`)
        }
      },
    },
  })
}
