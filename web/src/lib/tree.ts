import { call } from './ops'
import type { Entry, Kind } from './operations.gen'

/** Creates a Page or Folder at the end of its parent: the top of the Area when no parent is given. */
export function createEntry(kind: Kind, input: { area_id: string; parent_id?: string; title: string }): Promise<Entry> {
  return kind === 'folder' ? call('create_folder', input) : call('create_page', input)
}

/** Renames a Page or Folder. */
export function renameEntry(entry: Entry, title: string): Promise<Entry> {
  const input = { id: entry.id, title }
  return entry.kind === 'folder' ? call('rename_folder', input) : call('rename_page', input)
}

/** An Entry as one line of the sidebar's tree. */
export type Row = {
  entry: Entry
  /** How many Entries sit above it; the top of the Area is 0. */
  depth: number
  /** False when what is beneath it is folded away. */
  open: boolean
}

/**
 * The lines the sidebar shows for a tree, top to bottom. `collapsed` holds the
 * IDs of the Entries that are folded; what is beneath one of them is left out.
 */
export function visibleRows(entries: Entry[], collapsed: ReadonlySet<string>, depth = 0): Row[] {
  return entries.flatMap(entry => {
    const open = !collapsed.has(entry.id)
    return [{ entry, depth, open }, ...(open ? visibleRows(entry.children, collapsed, depth + 1) : [])]
  })
}

/** The IDs of the Entries that have something beneath them, which are the ones that can fold. */
export function foldable(entries: Entry[]): string[] {
  return entries.flatMap(entry => (entry.children.length > 0 ? [entry.id, ...foldable(entry.children)] : []))
}

/** The IDs of the Entries an Entry sits beneath, from the top of the Area down; none when it is at the top or not in the tree. */
export function above(entries: Entry[], id: string): string[] {
  for (const entry of entries) {
    if (entry.id === id) return []
    if (contains(entry.children, id)) return [entry.id, ...above(entry.children, id)]
  }
  return []
}

function contains(entries: Entry[], id: string): boolean {
  return entries.some(entry => entry.id === id || contains(entry.children, id))
}
