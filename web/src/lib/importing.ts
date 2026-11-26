/*
 * Importing a ZIP of Markdown files, such as an Export, or a Confluence
 * space exported as XML, into an Area or beneath an Entry. The file is sent
 * in base64, and what did not come across is told in a toast.
 */
import { toast } from 'svelte-sonner'
import { base64Of } from './attachments'
import type { Outcome } from './operations.gen'
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
  const imported = await send(file, content => call('import', { ...input, content }))
  if (!imported) return false
  const { problems } = imported
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

/**
 * Sends a ZIP in base64 with `importing`. A failure is told in a toast, and
 * then there is nothing.
 */
async function send<T>(file: File, importing: (content: string) => Promise<T>): Promise<T | null> {
  try {
    return await importing(base64Of(new Uint8Array(await file.arrayBuffer())))
  } catch (caught) {
    reportFailure(caught, `${file.name} could not be imported. Try again.`)
    return null
  }
}

/** How many kinds of content each line of the report names at most, before "and N more". */
const KINDS_PER_LINE = 6

/**
 * Imports a Confluence space exported as XML where `input` says, `into`
 * naming that place, and tells in a toast how much of it came across.
 * Returns whether anything was imported, so the tree is loaded again.
 */
export async function importConfluenceFromFile(
  input: Omit<Input<'import_confluence'>, 'content'>,
  file: File,
  into: string,
): Promise<boolean> {
  const imported = await send(file, content => call('import_confluence', { ...input, content }))
  if (!imported) return false
  const { coverage, problems } = imported
  const named = (outcome: Outcome) => {
    const found = coverage.elements.filter(element => element.outcome === outcome)
    const shown = found.slice(0, KINDS_PER_LINE).map(element => `${element.name} (${element.count})`)
    if (found.length > KINDS_PER_LINE) shown.push(`${found.length - KINDS_PER_LINE} more`)
    return shown.join(', ')
  }
  const lines = [
    `${coverage.pages} Pages, ${coverage.folders} Folders and ${coverage.attachments} Attachments. ${coverage.exact_pages} of the Pages came across exactly.`,
    `Of the content, ${coverage.exact_percent} % came across exactly and ${coverage.kept_percent} % at least approximately.`,
  ]
  for (const [outcome, label] of [
    ['left_out', 'Left out'],
    ['placeholder', 'Placeholders'],
    ['approximate', 'Approximately'],
  ] as const) {
    const names = named(outcome)
    if (names) lines.push(`${label}: ${names}.`)
  }
  if (coverage.not_imported.length > 0) {
    lines.push(`Not imported: ${coverage.not_imported.map(item => `${item.what} (${item.count})`).join(', ')}.`)
  }
  for (const problem of problems) lines.push(`Did not come across: ${problem.file}: ${problem.message}`)
  // Long enough to read the report, which says what to look into.
  toast(`Imported ${coverage.space} into ${into}`, { description: lines.join('\n'), duration: 30_000 })
  return true
}
