import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { importConfluenceFromFile, importFromFile, stageOf } from './importing'
import type { Coverage, ImportProgress } from './operations.gen'
import { OperationError } from './ops'

const call = vi.hoisted(() => vi.fn())
const callTellingUpload = vi.hoisted(() => vi.fn())
vi.mock('./ops', async original => ({ ...(await original<typeof import('./ops')>()), call, callTellingUpload }))
const toast = vi.hoisted(() =>
  Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn(), warning: vi.fn(), loading: vi.fn((_title: string, _options?: { id?: string; description: string }) => 'import-toast') }),
)
vi.mock('svelte-sonner', () => ({ toast }))
const session = vi.hoisted(() => ({ limits: { attachment_bytes: 25 * 1024 * 1024, import_bytes: 200 * 1024 * 1024 } }))
vi.mock('./session.svelte', () => ({ session }))
const router = vi.hoisted(() => ({ navigate: vi.fn() }))
vi.mock('./router.svelte', () => ({ router }))

beforeEach(() => {
  vi.clearAllMocks()
  vi.useFakeTimers()
  session.limits.import_bytes = 200 * 1024 * 1024
})
afterEach(() => vi.useRealTimers())

const zip = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'KB.zip', { type: 'application/zip' })
const MB = 1024 * 1024

function coverage(extra: Partial<Coverage> = {}): Coverage {
  return {
    space: 'Knowledge Base',
    pages: 86,
    folders: 22,
    attachments: 212,
    exact_pages: 53,
    exact_percent: 97,
    kept_percent: 99,
    elements: [
      { name: 'toc macro', outcome: 'placeholder', becomes: 'A placeholder.', count: 1, pages: ['Runbook'] },
      { name: 'missing attachment', outcome: 'left_out', becomes: 'Nothing.', count: 2, pages: ['Guide'] },
      { name: 'panel macro', outcome: 'approximate', becomes: 'A Callout.', count: 14, pages: ['Setup'] },
      { name: 'paragraph', outcome: 'exact', becomes: 'A paragraph.', count: 3323, pages: ['Home'] },
    ],
    not_imported: [
      { what: 'Comments', count: 5 },
      { what: 'Earlier versions of pages', count: 294 },
    ],
    too_large: [],
    attachment_limit_bytes: 25 * MB,
    ...extra,
  }
}

function progress(extra: Partial<ImportProgress>): ImportProgress {
  return { import_id: 'i1', stage: 'reading', done: 0, total: 0, ...extra }
}

/** Runs an import to its end, letting the time pass that it waits between asking. */
async function ended<T>(importing: Promise<T>): Promise<T> {
  await vi.runAllTimersAsync()
  return importing
}

test('a Confluence export is started where asked, followed until it is done, and the toast leads to the report Page', async () => {
  callTellingUpload.mockResolvedValue(progress({}))
  const imported = { import_id: 'i1', entries: [], problems: [], report_page_id: 'r1' }
  call
    .mockResolvedValueOnce(progress({ stage: 'creating', done: 40, total: 108, coverage: coverage() }))
    .mockResolvedValueOnce(progress({ stage: 'done', done: 86, total: 86, coverage: coverage(), imported }))

  const done = await ended(importConfluenceFromFile({ area_id: 'a1', parent_id: 'f1' }, zip, 'Archive'))

  expect(done).toBe(true)
  expect(callTellingUpload).toHaveBeenCalledWith('start_import_confluence', { area_id: 'a1', parent_id: 'f1', content: 'UEsDBA==' }, expect.any(Function))
  expect(call).toHaveBeenCalledWith('get_import_progress', { import_id: 'i1' })
  const [title, options] = toast.success.mock.calls[0] as [string, { id: string; description: string; duration: number; action: { label: string; onClick: () => void } }]
  expect(title).toBe('Imported Knowledge Base into Archive')
  expect(options.description).toBe(
    '97 % of the content came across exactly, 99 % at least approximately. 33 of the 86 Pages are to check.\nThe report beside the import lists them.',
  )
  expect(options.action.label).toBe('Open report')
  options.action.onClick()
  expect(router.navigate).toHaveBeenCalledWith('/p/r1/import-report')
})

test('the toast tells how much of the ZIP is uploaded and then how far the import has got', async () => {
  callTellingUpload.mockImplementation(async (_name, _input, uploaded: (share: number) => void) => {
    uploaded(0.456)
    return progress({})
  })
  call
    .mockResolvedValueOnce(progress({ stage: 'creating', done: 40, total: 108 }))
    .mockResolvedValueOnce(progress({ stage: 'writing', done: 12, total: 86 }))
    .mockResolvedValueOnce(progress({ stage: 'done', imported: { import_id: 'i1', entries: [], problems: [] } }))

  await ended(importFromFile({ area_id: 'a1' }, zip, 'Wiki'))

  const told = toast.loading.mock.calls.map(([, options]) => options?.description)
  expect(told).toEqual([
    'Preparing the upload…',
    'Uploading: 45 % of 4 bytes',
    'Reading the ZIP…',
    'Creating Pages and Folders: 40 of 108',
    "Writing the Pages' content: 12 of 86",
  ])
  expect(toast.success).toHaveBeenCalledWith('Imported KB.zip', { id: 'import-toast', description: 'Into Wiki' })
})

test('attachments too large for this Turtle are told as soon as the space is read, and once in the report', async () => {
  const tooLarge = coverage({ too_large: [{ file: 'Deploy/video.mp4', bytes: 31.5 * MB }] })
  const problems = [
    { file: 'Deploy/video.mp4', message: 'It is 31.5 MB, larger than 25 MB, …' },
    { file: 'Deploy/gone.pdf', message: 'Its file is not in the export, so it was left out.' },
  ]
  callTellingUpload.mockResolvedValue(progress({}))
  call.mockResolvedValueOnce(progress({ stage: 'done', coverage: tooLarge, imported: { import_id: 'i1', entries: [], problems } }))
  const toldLine =
    'Left out as larger than 25 MB, the most this Turtle accepts for one file: Deploy/video.mp4 (31.5 MB). Whoever runs Turtle can raise that limit with TURTLE_MAX_ATTACHMENT_MB.'

  await ended(importConfluenceFromFile({ area_id: 'a1' }, zip, 'Wiki'))

  expect(stageOf(progress({ stage: 'creating', done: 1, total: 9, coverage: tooLarge }))).toBe(`Creating Pages and Folders: 1 of 9\n${toldLine}`)
  const [, { description }] = toast.success.mock.calls[0] as [string, { description: string }]
  expect(description.split('\n')[1]).toBe('2 files did not come across. The report beside the import lists them and the Pages to check.')
})

test('a ZIP larger than this Turtle imports is refused before it is sent', async () => {
  session.limits.import_bytes = 3

  const imported = await ended(importConfluenceFromFile({ area_id: 'a1' }, zip, 'Wiki'))

  expect(imported).toBe(false)
  expect(callTellingUpload).not.toHaveBeenCalled()
  expect(toast.error).toHaveBeenCalledWith(
    'KB.zip is 4 bytes, larger than 3 bytes, the most this Turtle imports at once. Import it in parts, or ask whoever runs Turtle to raise the limit with TURTLE_MAX_IMPORT_MB.',
  )
})

test('when the import cannot start, nothing is reported as imported and the toast says why', async () => {
  callTellingUpload.mockRejectedValue(new OperationError('invalid_input', 'This ZIP is larger than 200 MB, the most this Turtle imports at once.'))

  const imported = await ended(importConfluenceFromFile({ area_id: 'a1' }, zip, 'Wiki'))

  expect(imported).toBe(false)
  expect(toast.error).toHaveBeenCalledWith('This ZIP is larger than 200 MB, the most this Turtle imports at once.', { id: 'import-toast' })
})

test('when the import fails while running, nothing is reported as imported and the toast says why', async () => {
  callTellingUpload.mockResolvedValue(progress({}))
  call.mockResolvedValueOnce(progress({ stage: 'failed', error: 'This is Confluence’s CSV export, which Turtle cannot read yet.' }))

  const imported = await ended(importConfluenceFromFile({ area_id: 'a1' }, zip, 'Wiki'))

  expect(imported).toBe(false)
  expect(toast.error).toHaveBeenCalledWith('This is Confluence’s CSV export, which Turtle cannot read yet.', { id: 'import-toast' })
})

test('an import that fails once it has created Pages says that they are kept and how to undo them', async () => {
  callTellingUpload.mockResolvedValue(progress({}))
  call
    .mockResolvedValueOnce(progress({ stage: 'creating', done: 3, total: 9 }))
    .mockResolvedValueOnce(progress({ stage: 'failed', error: 'Something went wrong.' }))

  await ended(importConfluenceFromFile({ area_id: 'a1' }, zip, 'Wiki'))

  expect(toast.error).toHaveBeenCalledWith(
    'Something went wrong. What it created before is kept; to undo it, choose Imports in the menu of the Area.',
    { id: 'import-toast' },
  )
})

test('what did not come across from Markdown is named file by file', async () => {
  const problems = [{ file: 'Runbook/_attachments/big.bin', message: 'It is 30 MB, larger than 25 MB, …' }]
  callTellingUpload.mockResolvedValue(progress({}))
  call.mockResolvedValueOnce(progress({ stage: 'done', imported: { import_id: 'i1', entries: [], problems } }))

  await ended(importFromFile({ area_id: 'a1' }, zip, 'Wiki'))

  expect(toast.warning).toHaveBeenCalledWith('Imported KB.zip, but one file did not come across', {
    id: 'import-toast',
    description: 'Runbook/_attachments/big.bin: It is 30 MB, larger than 25 MB, …',
    duration: 30_000,
  })
})
