/*
 * Importing a ZIP of Markdown files, such as an Export, or a Confluence
 * space exported as CSV or XML, into an Area or beneath an Entry. A ZIP larger
 * than this Turtle imports is refused before it is sent. Otherwise it is
 * sent in base64 to start the import, which then runs on the server while a
 * toast tells how far the upload and then the import have got; at the end,
 * the toast tells what did not come across, and for Confluence leads to the
 * report Page.
 */
import { toast } from 'svelte-sonner'
import { base64Of, fileSize } from './attachments'
import type { Coverage, ImportProgress, Imported } from './operations.gen'
import { call, callTellingUpload, OperationError, type Input } from './ops'
import { router } from './router.svelte'
import { REPORT_LABEL } from './imports'
import { pageAddress } from './routes'
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

/**
 * Imports a Confluence space exported as CSV or XML where `input` says, `into`
 * naming that place. The report on it is a Page beside the import; the
 * toast tells how much came across and leads there.
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
  const toCheck = coverage.pages - coverage.exact_pages
  const lines = [
    `${coverage.exact_percent} % of the content came across exactly, ${coverage.kept_percent} % at least approximately. ` +
      (toCheck === 0 ? 'Every Page came across exactly.' : `${toCheck} of the ${coverage.pages} Pages are to check.`),
  ]
  const files = imported.problems.length
  const what = toCheck === 0 ? 'them' : 'them and the Pages to check'
  lines.push(
    files === 0
      ? `The report beside the import ${toCheck === 0 ? 'tells more' : 'lists them'}.`
      : `${files === 1 ? 'One file' : `${files} files`} did not come across. The report beside the import lists ${what}.`,
  )
  const report = imported.report_page_id
  toast.success(`Imported ${coverage.space} into ${into}`, {
    id: ended.toast,
    description: lines.join('\n'),
    duration: 15_000,
    action: report ? { label: 'Open report', onClick: () => router.navigate(pageAddress(report, REPORT_LABEL)) } : undefined,
  })
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
  /** Whether it got as far as creating Pages and Folders, which a failure keeps. */
  let creating = false
  while (progress.stage !== 'done' && progress.stage !== 'failed') {
    creating ||= progress.stage === 'creating' || progress.stage === 'writing'
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
    const failed = progress.error ?? `${file.name} could not be imported. Try again.`
    // What it created before it failed is kept, and can be undone.
    const kept = creating ? ' What it created before is kept; to undo it, choose Imports in the menu of the Area.' : ''
    toast.error(failed + kept, { id })
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
