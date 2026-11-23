import { beforeEach, expect, test, vi } from 'vitest'
import { copyLink, entryMenuItems } from './entryMenu'

const toast = vi.hoisted(() => vi.fn())
vi.mock('svelte-sonner', () => ({ toast }))

beforeEach(() => toast.mockClear())

test('with Write or Admin, a Page offers creating beneath it, renaming, moving, copying its link and moving it to the Trash', () => {
  expect(entryMenuItems('page', 'write')).toEqual(['new-page', 'new-folder', 'rename', 'move', 'copy-link', 'trash'])
  expect(entryMenuItems('page', 'admin')).toEqual(['new-page', 'new-folder', 'rename', 'move', 'copy-link', 'trash'])
})

test('with only Read, a Page offers only copying its link', () => {
  expect(entryMenuItems('page', 'read')).toEqual(['copy-link'])
})

test('a Folder has no address of its own, so it offers no link', () => {
  expect(entryMenuItems('folder', 'write')).toEqual(['new-page', 'new-folder', 'rename', 'move', 'trash'])
  expect(entryMenuItems('folder', 'read')).toEqual([])
})

const page = { id: 'p1', title: 'Deploying to production' }

test('copying a link puts the Page’s full address on the clipboard and says so', async () => {
  const writeText = vi.fn(async () => {})
  await copyLink(page, 'https://turtle.example', { writeText })
  expect(writeText).toHaveBeenCalledWith('https://turtle.example/p/p1/deploying-to-production')
  expect(toast).toHaveBeenCalledWith('Link copied', { description: 'Deploying to production' })
})

test('when the clipboard refuses, the address is shown so it can be copied by hand', async () => {
  const writeText = vi.fn(async () => Promise.reject(new Error('denied')))
  await copyLink(page, 'https://turtle.example', { writeText })
  expect(toast).toHaveBeenCalledWith('The link could not be copied. Copy it from here.', {
    description: 'https://turtle.example/p/p1/deploying-to-production',
  })
})

test('without a clipboard at all, the address is shown as well', async () => {
  await copyLink(page, 'https://turtle.example', undefined)
  expect(toast).toHaveBeenCalledWith('The link could not be copied. Copy it from here.', {
    description: 'https://turtle.example/p/p1/deploying-to-production',
  })
})
