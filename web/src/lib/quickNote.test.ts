import { expect, test } from 'vitest'
import * as Y from 'yjs'
import { docFrom, stateOf, unsavedQuickNote, type UnsavedStorage } from './quickNote'

function memoryStorage(): UnsavedStorage {
  const items = new Map<string, string>()
  return {
    getItem: key => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: key => void items.delete(key),
  }
}

test('a draft one Member has not saved comes back after a reload, and not for another Member', () => {
  const storage = memoryStorage()
  unsavedQuickNote('grace', storage).keep({ title: 'Call the printer', body: 'AAA=' })

  expect(unsavedQuickNote('grace', storage).read()).toEqual({ title: 'Call the printer', body: 'AAA=' })
  expect(unsavedQuickNote('ken', storage).read()).toEqual({ title: '', body: '' })
})

test('an emptied draft leaves nothing behind', () => {
  const storage = memoryStorage()
  const unsaved = unsavedQuickNote('grace', storage)
  unsaved.keep({ title: 'Idea', body: '' })
  unsaved.keep({ title: '  ', body: '' })

  expect(storage.getItem('turtle.unsaved-quick-note.grace')).toBeNull()
})

test('text kept before Quick Notes had a title of their own is not mistaken for a draft', () => {
  const storage = memoryStorage()
  storage.setItem('turtle.unsaved-quick-note.grace', 'Call the printer\nAbout the invoice')

  expect(unsavedQuickNote('grace', storage).read()).toEqual({ title: '', body: '' })
})

test('storage that refuses, as in a private window, loses the draft but nothing else', () => {
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

  expect(() => unsaved.keep({ title: 'Idea', body: '' })).not.toThrow()
  expect(unsaved.read()).toEqual({ title: '', body: '' })
})

test('a body document comes back whole from its state, and from nonsense as an empty one', () => {
  const doc = new Y.Doc()
  const paragraph = new Y.XmlElement('paragraph')
  paragraph.insert(0, [new Y.XmlText('Ask about the invoice')])
  doc.getXmlFragment('default').insert(0, [paragraph])

  const back = docFrom(stateOf(doc))

  expect(back.getXmlFragment('default').toString()).toBe('<paragraph>Ask about the invoice</paragraph>')
  expect(docFrom('not a state').getXmlFragment('default').length).toBe(0)
})
