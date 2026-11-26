import { expect, test } from 'vitest'
import * as Y from 'yjs'
import { docFrom, quickNoteTitle, stateOf, unsavedQuickNote, type UnsavedStorage } from './quickNote'

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

test('a typed title is what a Quick Note is saved under', () => {
  expect(quickNoteTitle('  Standup  ', 'Ask about the release')).toBe('Standup')
})

test('without a typed title, the first line becomes the title', () => {
  expect(quickNoteTitle('', 'Ask   about the\trelease')).toBe('Ask about the release')
  expect(quickNoteTitle('   ', 'Call the printer people')).toBe('Call the printer people')
})

test('a long first line is cut at the last word that fits', () => {
  const line =
    'The printer on the second floor jams whenever someone sends a document with more than forty pages to it at once'
  expect(quickNoteTitle('', line)).toBe('The printer on the second floor jams whenever someone sends a document with…')
})

test('with neither a title nor a line there is nothing to save', () => {
  expect(quickNoteTitle(' ', '  ')).toBe('')
})
