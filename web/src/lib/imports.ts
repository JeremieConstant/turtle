/*
 * The imports into an Area, kept so they can be looked at and undone.
 * Undoing moves what an import created to the Trash, after asking.
 */
import { toast } from 'svelte-sonner'
import { ask } from './confirm.svelte'
import type { Author, ImportRecord } from './operations.gen'
import { call } from './ops'
import { reportFailure } from './toasts'

/** The imports into an Area, newest first. */
export async function importsOf(areaId: string): Promise<ImportRecord[]> {
  return (await call('list_imports', { area_id: areaId })).imports
}

/** The label a report Page's address is given; the ID alone finds the Page. */
export const REPORT_LABEL = 'Import report'

/** A number of Pages and Folders, in words: "1 Page or Folder", "12 Pages and Folders". */
export function pagesAndFolders(count: number): string {
  return count === 1 ? '1 Page or Folder' : `${count} Pages and Folders`
}

/** Who started an import, as people read it; empty once they no longer exist. */
export function startedBy(author: Author | null | undefined): string {
  if (!author) return ''
  return author.kind === 'agent_for_member' ? `${author.name} for ${author.member_name}` : author.name
}

/**
 * Undoes an import once the person confirms, and tells in a toast what
 * went to the Trash. Returns whether it was undone, so the tree is loaded
 * again.
 */
export async function undoImport(record: ImportRecord): Promise<boolean> {
  const yes = await ask({
    title: `Undo the import of ${record.source}?`,
    message: `The ${pagesAndFolders(record.created_count)} it created that are still there go to the Trash, with whatever was put beneath them since. Each can be restored from the Trash. Its report stays, marked as undone.`,
    action: 'Undo import',
    destructive: true,
  })
  if (!yes) return false
  let undone
  try {
    undone = await call('undo_import', { import_id: record.id })
  } catch (caught) {
    reportFailure(caught, 'The import could not be undone. Try again.')
    return false
  }
  const lines = [`${pagesAndFolders(undone.trashed)} went to the Trash, from where each can be restored.`]
  if (undone.taken_along.length > 0) {
    lines.push(`Put beneath them since, and in the Trash with them: ${undone.taken_along.join(', ')}.`)
  }
  toast(`Undid the import of ${record.source}`, { description: lines.join('\n') })
  return true
}
