import { expect, test } from 'vitest'
import { unsavedQuickNote, type UnsavedStorage } from './quickNote'

function memoryStorage(): UnsavedStorage {
  const items = new Map<string, string>()
  return {
    getItem: key => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: key => void items.delete(key),
  }
}

test('text one Member has not saved comes back after a reload, and not for another Member', () => {
  const storage = memoryStorage()
  unsavedQuickNote('grace', storage).keep('Call the printer\nAbout the invoice')

  expect(unsavedQuickNote('grace', storage).read()).toBe('Call the printer\nAbout the invoice')
  expect(unsavedQuickNote('ken', storage).read()).toBe('')
})

test('an emptied box leaves nothing behind', () => {
  const storage = memoryStorage()
  const unsaved = unsavedQuickNote('grace', storage)
  unsaved.keep('Idea')
  unsaved.keep('  ')

  expect(unsavedQuickNote('grace', storage).read()).toBe('')
})

test('storage that refuses, as in a private window, loses the unsaved text but nothing else', () => {
  const refusing: UnsavedStorage = {
    getItem: () => {
      throw new Error('denied')
    },
    setItem: () => {
      throw new Error('denied')
    },
    removeItem: () => {
      throw new Error('denied')
    },
  }
  const unsaved = unsavedQuickNote('grace', refusing)

  expect(() => unsaved.keep('Idea')).not.toThrow()
  expect(unsaved.read()).toBe('')
})
