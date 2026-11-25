/*
 * Importing a ZIP of Markdown files, such as an Export, into an Area or
 * beneath an Entry. The file is sent in base64, and what did not come
 * across is told in a toast, file by file.
 */
import { toast } from 'svelte-sonner'
import { base64Of } from './attachments'
import { call, type Input } from './ops'
import { reportFailure } from './toasts'

/** Lets the person choose a ZIP; null if they close the picker without one. */
export function chooseZip(): Promise<File | null> {
  return new Promise(resolve => {
    const picker = document.createElement('input')
    picker.type = 'file'
    picker.accept = '.zip,application/zip'
    picker.addEventListener('change', () => resolve(picker.files?.[0] ?? null))
    picker.addEventListener('cancel', () => resolve(null))
    picker.click()
  })
}

/**
 * Imports a ZIP where `input` says, `into` naming that place for the toast.
 * Returns whether anything was imported, so the tree is loaded again.
 */
export async function importFromFile(input: Omit<Input<'import'>, 'content'>, file: File, into: string): Promise<boolean> {
  let problems
  try {
    const content = base64Of(new Uint8Array(await file.arrayBuffer()))
    ;({ problems } = await call('import', { ...input, content }))
  } catch (caught) {
    reportFailure(caught, `${file.name} could not be imported. Try again.`)
    return false
  }
  if (problems.length === 0) {
    toast(`Imported ${file.name}`, { description: `Into ${into}` })
  } else {
    const files = problems.length === 1 ? 'One file' : `${problems.length} files`
    toast(`Imported ${file.name}, but ${files} did not come across`, {
      description: problems.map(problem => `${problem.file}: ${problem.message}`).join('\n'),
      // Long enough to read the list, which says what to look into.
      duration: 30_000,
    })
  }
  return true
}
