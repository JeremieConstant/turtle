import { beforeEach, expect, test, vi } from 'vitest'
import { exportToFile } from './exporting'
import { OperationError } from './ops'

const call = vi.hoisted(() => vi.fn())
vi.mock('./ops', async original => ({ ...(await original<typeof import('./ops')>()), call }))
const toast = vi.hoisted(() => Object.assign(vi.fn(), { error: vi.fn() }))
vi.mock('svelte-sonner', () => ({ toast }))

beforeEach(() => vi.clearAllMocks())

test('an export is saved as a ZIP under the name Turtle gives it', async () => {
  // "PK" and two more bytes, as a ZIP starts, in base64.
  call.mockResolvedValue({ file_name: 'Runbook.zip', content: 'UEsDBA==' })
  const save = vi.fn()

  await exportToFile({ area_id: 'a1', entry_id: 'p1' }, 'Runbook', save)

  expect(call).toHaveBeenCalledWith('export', { area_id: 'a1', entry_id: 'p1' })
  const [name, file] = save.mock.calls[0] as [string, Blob]
  expect(name).toBe('Runbook.zip')
  expect(file.type).toBe('application/zip')
  expect([...new Uint8Array(await file.arrayBuffer())]).toEqual([0x50, 0x4b, 0x03, 0x04])
})

test('when the export fails, nothing is saved and a toast says why', async () => {
  call.mockRejectedValue(new OperationError('not_found', 'There is no such Page or Folder.'))
  const save = vi.fn()

  await exportToFile({ area_id: 'a1', entry_id: 'p1' }, 'Runbook', save)

  expect(save).not.toHaveBeenCalled()
  expect(toast.error).toHaveBeenCalledWith('There is no such Page or Folder.')
})
