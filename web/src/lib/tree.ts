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
