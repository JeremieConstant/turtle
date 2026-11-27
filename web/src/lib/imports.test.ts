import { beforeEach, expect, test, vi } from 'vitest'
import { startedBy, undoImport } from './imports'
import type { ImportRecord } from './operations.gen'
import { OperationError } from './ops'

const call = vi.hoisted(() => vi.fn())
vi.mock('./ops', async original => ({ ...(await original<typeof import('./ops')>()), call }))
const ask = vi.hoisted(() => vi.fn())
vi.mock('./confirm.svelte', () => ({ ask }))
const toast = vi.hoisted(() => Object.assign(vi.fn(), { error: vi.fn() }))
vi.mock('svelte-sonner', () => ({ toast }))

beforeEach(() => vi.clearAllMocks())

const record: ImportRecord = {
  id: 'i1',
  source: 'the Confluence space Knowledge Base',
  started_at: 1_700_000_000,
  started_by: { kind: 'member', id: 'm1', name: 'Grace' },
  created_count: 109,
  report_page_id: 'r1',
  undoable: true,
  undone_at: null,
}

test('an import is undone once the person confirms, and the toast tells what went to the Trash', async () => {
  ask.mockResolvedValue(true)
  call.mockResolvedValue({ trashed: 109, taken_along: ['My checklist'] })

  const undone = await undoImport(record)

  expect(undone).toBe(true)
  expect(ask).toHaveBeenCalledWith(expect.objectContaining({ title: 'Undo the import of the Confluence space Knowledge Base?', action: 'Undo import', destructive: true }))
  expect(call).toHaveBeenCalledWith('undo_import', { import_id: 'i1' })
  expect(toast).toHaveBeenCalledWith('Undid the import of the Confluence space Knowledge Base', {
    description: '109 Pages and Folders went to the Trash, from where each can be restored.\nPut beneath them since, and in the Trash with them: My checklist.',
  })
})

test('nothing is undone when the person does not confirm', async () => {
  ask.mockResolvedValue(false)

  expect(await undoImport(record)).toBe(false)
  expect(call).not.toHaveBeenCalled()
})

test('when undoing fails, the toast says why', async () => {
  ask.mockResolvedValue(true)
  call.mockRejectedValue(new OperationError('conflict', 'This import is undone already.'))

  expect(await undoImport(record)).toBe(false)
  expect(toast.error).toHaveBeenCalledWith('This import is undone already.')
})

test('one Page reads as one, more as many', async () => {
  const { pagesAndFolders } = await import('./imports')
  expect(pagesAndFolders(1)).toBe('1 Page or Folder')
  expect(pagesAndFolders(0)).toBe('0 Pages and Folders')
})

test('who started an import reads as their name, an Agent acting for someone as both', () => {
  expect(startedBy(record.started_by)).toBe('Grace')
  expect(startedBy({ kind: 'agent_for_member', name: 'CI', member_id: 'm1', member_name: 'Grace' })).toBe('CI for Grace')
  expect(startedBy(null)).toBe('')
})
