import { expect, test } from 'vitest'
import { asksForSearch, latestOnly, moveMark, place } from './search'

/** A search whose answers arrive when the test says so. */
function controlled() {
  const pending: Array<{ query: string; answer: (results: string[]) => void }> = []
  const run = (query: string) => new Promise<string[]>(answer => pending.push({ query, answer }))
  return { run, pending }
}

test('an answer to an earlier query that arrives late is dropped', async () => {
  const { run, pending } = controlled()
  const search = latestOnly(run)
  const shown: string[][] = []

  const first = search('roa').then(results => results && shown.push(results))
  const second = search('road').then(results => results && shown.push(results))
  pending[1].answer(['Roadmap'])
  pending[0].answer(['Roadmap', 'Road trip'])
  await Promise.all([first, second])

  expect(shown).toEqual([['Roadmap']])
})

test('answers that arrive in order are all kept', async () => {
  const { run, pending } = controlled()
  const search = latestOnly(run)

  const first = search('roa')
  pending[0].answer(['Roadmap'])
  expect(await first).toEqual(['Roadmap'])
})

test('a result is placed by its Area and the Pages and Folders above it', () => {
  expect(place({ area_name: 'Engineering', path: ['Guides', 'Setup'] })).toBe('Engineering › Guides › Setup')
  expect(place({ area_name: 'Engineering', path: [] })).toBe('Engineering')
})

test('the arrow keys move the mark through the results and around the ends', () => {
  expect(moveMark(0, 3, 'ArrowDown')).toBe(1)
  expect(moveMark(1, 3, 'ArrowDown')).toBe(2)
  expect(moveMark(2, 3, 'ArrowDown')).toBe(0)
  expect(moveMark(2, 3, 'ArrowUp')).toBe(1)
  expect(moveMark(0, 3, 'ArrowUp')).toBe(2)
})

test('without results the mark stays where it is', () => {
  expect(moveMark(0, 0, 'ArrowDown')).toBe(0)
  expect(moveMark(0, 0, 'ArrowUp')).toBe(0)
})

const key = (key: string, held: Partial<Record<'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey', boolean>> = {}) => ({
  key,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...held,
})

test('a slash outside a text field asks for the search', () => {
  expect(asksForSearch(key('/'), false, true)).toBe(true)
})

test('a slash typed into a text field is only a slash', () => {
  expect(asksForSearch(key('/'), true, true)).toBe(false)
})

test('other keys, and a slash that belongs to a shortcut of the browser, do not ask for the search', () => {
  expect(asksForSearch(key('s'), false, true)).toBe(false)
  expect(asksForSearch(key('/', { metaKey: true }), false, true)).toBe(false)
  expect(asksForSearch(key('/', { ctrlKey: true }), false, true)).toBe(false)
  expect(asksForSearch(key('/', { altKey: true }), false, true)).toBe(false)
})

test('⌘K on a Mac asks for the search, even while typing', () => {
  expect(asksForSearch(key('k', { metaKey: true }), false, true)).toBe(true)
  expect(asksForSearch(key('k', { metaKey: true }), true, true)).toBe(true)
  expect(asksForSearch(key('K', { metaKey: true }), true, true)).toBe(true)
})

test('off a Mac it is Ctrl+K', () => {
  expect(asksForSearch(key('k', { ctrlKey: true }), true, false)).toBe(true)
  expect(asksForSearch(key('k', { metaKey: true }), false, false)).toBe(false)
  expect(asksForSearch(key('k', { ctrlKey: true }), false, true)).toBe(false)
})

test('K alone or with another modifier is left to whatever else uses it', () => {
  expect(asksForSearch(key('k'), false, true)).toBe(false)
  expect(asksForSearch(key('k', { metaKey: true, shiftKey: true }), false, true)).toBe(false)
  expect(asksForSearch(key('k', { metaKey: true, altKey: true }), false, true)).toBe(false)
})
