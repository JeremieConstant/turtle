import { afterEach, expect, test, vi } from 'vitest'
import { currentPerson, personColor } from './person'

afterEach(() => vi.unstubAllGlobals())

function fakeSessionStorage(entries: Record<string, string> = {}) {
  const stored = new Map(Object.entries(entries))
  return {
    getItem: (key: string) => stored.get(key) ?? null,
    setItem: (key: string, value: string) => void stored.set(key, value),
    stored,
  }
}

test('a person is sent with the number of their colour, not the colour itself', () => {
  const storage = fakeSessionStorage()
  vi.stubGlobal('sessionStorage', storage)
  const person = currentPerson('Ada')
  expect(person.name).toBe('Ada')
  expect([1, 2, 3, 4, 5, 6]).toContain(person.colorNumber)
  expect('color' in person).toBe(false)
})

test('the colour lasts for the browser session', () => {
  vi.stubGlobal('sessionStorage', fakeSessionStorage({ 'turtle-color-number': '4' }))
  expect(currentPerson('Ada').colorNumber).toBe(4)
})

test('a kept value that is not a colour number is replaced by one', () => {
  const storage = fakeSessionStorage({ 'turtle-color-number': '#d9480f' })
  vi.stubGlobal('sessionStorage', storage)
  const { colorNumber } = currentPerson('Ada')
  expect([1, 2, 3, 4, 5, 6]).toContain(colorNumber)
  expect(storage.stored.get('turtle-color-number')).toBe(String(colorNumber))
})

test('everyone shows a colour number as the same named colour of their own appearance', () => {
  expect(personColor({ colorNumber: 3 })).toBe('var(--color-person-3)')
  expect(personColor({ colorNumber: 6 })).toBe('var(--color-person-6)')
})

test('anything else that arrives from another browser is shown in a quiet colour', () => {
  expect(personColor({ colorNumber: 7 })).toBe('var(--color-muted)')
  expect(personColor({ colorNumber: 0 })).toBe('var(--color-muted)')
  expect(personColor({ colorNumber: '2); background: url(x)' })).toBe('var(--color-muted)')
  expect(personColor({})).toBe('var(--color-muted)')
})
