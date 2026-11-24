import { afterEach, expect, test, vi } from 'vitest'
import { currentPerson, personColor, pictureOf, presenceLabel } from './person'

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

test('an app acting for a Member is named the way its changes are credited', () => {
  expect(presenceLabel({ name: 'Claude', agent: true, member: 'Ada' })).toBe('Ada via Claude')
})

test('a standalone Agent is recognisable as one', () => {
  expect(presenceLabel({ name: 'Release notes writer', agent: true })).toBe('Release notes writer (Agent)')
})

test('a person is sent with their ID, so the others can show their picture', () => {
  vi.stubGlobal('sessionStorage', fakeSessionStorage())
  expect(currentPerson('Ada', 'member-ada').id).toBe('member-ada')
})

test('a person present shows the picture of the Profile with their ID', () => {
  const profiles = [
    { id: 'member-ada', picture: '/api/profile_pictures/member-ada?v=1' },
    { id: 'member-ken', picture: null },
  ]
  expect(pictureOf({ id: 'member-ada' }, profiles)).toBe('/api/profile_pictures/member-ada?v=1')
  expect(pictureOf({ id: 'member-ken' }, profiles)).toBeNull()
})

test('a person present whose Profile the viewer does not see, or who sent no ID, shows no picture', () => {
  const profiles = [{ id: 'member-ada', picture: '/api/profile_pictures/member-ada?v=1' }]
  expect(pictureOf({ id: 'member-gus' }, profiles)).toBeNull()
  expect(pictureOf({}, profiles)).toBeNull()
  // What another browser sends is not trusted.
  expect(pictureOf({ id: ['member-ada'] }, profiles)).toBeNull()
})
