import { beforeEach, expect, test, vi } from 'vitest'
import { importConfluenceFromFile } from './importing'
import type { Coverage } from './operations.gen'
import { OperationError } from './ops'

const call = vi.hoisted(() => vi.fn())
vi.mock('./ops', async original => ({ ...(await original<typeof import('./ops')>()), call }))
const toast = vi.hoisted(() => Object.assign(vi.fn(), { error: vi.fn() }))
vi.mock('svelte-sonner', () => ({ toast }))

beforeEach(() => vi.clearAllMocks())

const zip = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'KB.zip', { type: 'application/zip' })

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
    ...extra,
  }
}

test('a Confluence export is imported where asked and the report is told in a toast', async () => {
  call.mockResolvedValue({ entries: [], problems: [], coverage: coverage() })

  const imported = await importConfluenceFromFile({ area_id: 'a1', parent_id: 'f1' }, zip, 'Archive')

  expect(imported).toBe(true)
  expect(call).toHaveBeenCalledWith('import_confluence', { area_id: 'a1', parent_id: 'f1', content: 'UEsDBA==' })
  expect(toast).toHaveBeenCalledWith('Imported Knowledge Base into Archive', {
    description: [
      '86 Pages, 22 Folders and 212 Attachments. 53 of the Pages came across exactly.',
      'Of the content, 97 % came across exactly and 99 % at least approximately.',
      'Left out: missing attachment (2).',
      'Placeholders: toc macro (1).',
      'Approximately: panel macro (14).',
      'Not imported: Comments (5), Earlier versions of pages (294).',
    ].join('\n'),
    duration: 30_000,
  })
})

test('what did not come across is named file by file', async () => {
  const problems = [{ file: 'Deploy/gone.pdf', message: 'Its file is not in the export, so it was left out.' }]
  call.mockResolvedValue({ entries: [], problems, coverage: coverage({ elements: [], not_imported: [] }) })

  await importConfluenceFromFile({ area_id: 'a1' }, zip, 'Wiki')

  const [, { description }] = toast.mock.calls[0] as [string, { description: string }]
  expect(description.split('\n').at(-1)).toBe('Did not come across: Deploy/gone.pdf: Its file is not in the export, so it was left out.')
})

test('when the import fails, nothing is reported as imported and a toast says why', async () => {
  call.mockRejectedValue(new OperationError('invalid_input', 'This is Confluence’s CSV export, which Turtle cannot read yet.'))

  const imported = await importConfluenceFromFile({ area_id: 'a1' }, zip, 'Wiki')

  expect(imported).toBe(false)
  expect(toast.error).toHaveBeenCalledWith('This is Confluence’s CSV export, which Turtle cannot read yet.')
})
