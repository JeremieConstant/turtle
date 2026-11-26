/*
 * Importing a ZIP of Markdown files, such as an Export, or a Confluence
 * space exported as XML, into an Area or beneath an Entry. A ZIP larger
 * than this Turtle imports is refused before it is sent. Otherwise it is
 * sent in base64 to start the import, which then runs on the server while a
 * toast tells how far the upload and then the import have got; at the end,
 * the toast tells what did not come across.
 */
import { toast } from 'svelte-sonner'
import { base64Of, fileSize } from './attachments'
import type { Coverage, ImportProgress, Imported, Outcome } from './operations.gen'
import { call, callTellingUpload, OperationError, type Input } from './ops'
import { session } from './session.svelte'

/** How long to wait between asking how far an import has got, in milliseconds. */
export const ASKED_EVERY = 1000

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
  const ended = await run(file, (content, uploaded) => callTellingUpload('start_import', { ...input, content }, uploaded))
  if (!ended) return false
  const { problems } = ended.imported
  if (problems.length === 0) {
    toast.success(`Imported ${file.name}`, { id: ended.toast, description: `Into ${into}` })
  } else {
    const files = problems.length === 1 ? 'one file' : `${problems.length} files`
    toast.warning(`Imported ${file.name}, but ${files} did not come across`, {
      id: ended.toast,
      description: problems.map(problem => `${problem.file}: ${problem.message}`).join('\n'),
      // Long enough to read the list, which says what to look into.
      duration: 30_000,
    })
  }
  return true
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
  const ended = await run(file, (content, uploaded) => callTellingUpload('start_import_confluence', { ...input, content }, uploaded))
  if (!ended?.coverage) return false
  const { coverage, imported } = ended
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
  const tooLarge = tooLargeLine(coverage)
  if (tooLarge) lines.push(tooLarge)
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
  // The files too large are told once above, not again one by one.
  const tooLargeFiles = new Set(coverage.too_large.map(file => file.file))
  for (const problem of imported.problems) {
    if (!tooLargeFiles.has(problem.file)) lines.push(`Did not come across: ${problem.file}: ${problem.message}`)
  }
  // Long enough to read the report, which says what to look into.
  toast.success(`Imported ${coverage.space} into ${into}`, { id: ended.toast, description: lines.join('\n'), duration: 30_000 })
  return true
}

/** The attachments a space holds that are larger than this Turtle accepts, in one line; empty if there are none. */
function tooLargeLine(coverage: Coverage): string {
  if (coverage.too_large.length === 0) return ''
  const files = coverage.too_large.map(file => `${file.file} (${fileSize(file.bytes)})`).join(', ')
  return `Left out as larger than ${fileSize(coverage.attachment_limit_bytes)}, the most this Turtle accepts for one file: ${files}. Whoever runs Turtle can raise that limit with TURTLE_MAX_ATTACHMENT_MB.`
}

/** An import that is done, with the toast that followed it, to tell the outcome in. */
type Ended = { toast: string | number; imported: Imported; coverage: Coverage | null }

/**
 * Starts an import of `file` with `start`, which sends the ZIP in base64 and
 * tells what share of it is sent, and follows it until it ends, telling how
 * far it has got in a toast. A ZIP larger than this Turtle imports is not
 * sent. A failure is told in the toast, and then there is nothing.
 */
async function run(
  file: File,
  start: (content: string, uploaded: (share: number) => void) => Promise<ImportProgress>,
): Promise<Ended | null> {
  const limit = session.limits?.import_bytes
  if (limit !== undefined && file.size > limit) {
    toast.error(
      `${file.name} is ${fileSize(file.size)}, larger than ${fileSize(limit)}, the most this Turtle imports at once. Import it in parts, or ask whoever runs Turtle to raise the limit with TURTLE_MAX_IMPORT_MB.`,
    )
    return null
  }
  const title = `Importing ${file.name}`
  const id = toast.loading(title, { description: 'Preparing the upload…' })
  let progress: ImportProgress
  try {
    const content = base64Of(new Uint8Array(await file.arrayBuffer()))
    progress = await start(content, share => {
      toast.loading(title, { id, description: `Uploading: ${Math.floor(share * 100)} % of ${fileSize(file.size)}` })
    })
  } catch (caught) {
    toast.error(caught instanceof OperationError ? caught.message : `${file.name} could not be imported. Try again.`, { id })
    return null
  }
  while (progress.stage !== 'done' && progress.stage !== 'failed') {
    toast.loading(title, { id, description: stageOf(progress) })
    await new Promise(resolve => setTimeout(resolve, ASKED_EVERY))
    try {
      progress = await call('get_import_progress', { import_id: progress.import_id })
    } catch {
      // The import goes on without anyone asking.
      toast.error(`Turtle could not be asked how far the import of ${file.name} has got. It may still be running: look where it goes in a while.`, {
        id,
      })
      return null
    }
  }
  if (progress.stage === 'failed' || !progress.imported) {
    toast.error(progress.error ?? `${file.name} could not be imported. Try again.`, { id })
    return null
  }
  return { toast: id, imported: progress.imported, coverage: progress.coverage ?? null }
}

/** How far a running import has got, as the toast tells it. */
export function stageOf(progress: ImportProgress): string {
  const lines = []
  switch (progress.stage) {
    case 'reading':
      lines.push('Reading the ZIP…')
      break
    case 'creating':
      lines.push(`Creating Pages and Folders: ${progress.done} of ${progress.total}`)
      break
    case 'writing':
      lines.push(`Writing the Pages' content: ${progress.done} of ${progress.total}`)
      break
  }
  // What will not come across is told as soon as it is known.
  const tooLarge = progress.coverage ? tooLargeLine(progress.coverage) : ''
  if (tooLarge) lines.push(tooLarge)
  return lines.join('\n')
}
