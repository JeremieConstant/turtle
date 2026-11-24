import { expect, test, vi } from 'vitest'
import type { Profile } from './operations.gen'

// A Mention is followed through the router, which reads the address bar as it loads.
vi.mock('./router.svelte', () => ({ router: {} }))

const { badgeOf, choices, personIdOf } = await import('./mention')

function person(name: string, changes: Partial<Profile> = {}): Profile {
  const id = name.toLowerCase().replace(/\s+/g, '-')
  return { id, name, email: `${id}@example.org`, guest: false, workspace_admin: false, active: true, picture: null, ...changes }
}

test('a Mention is written as turtle:member/ and the ID, as the server reads it', () => {
  expect(personIdOf('turtle:member/a1b2')).toBe('a1b2')
  expect(personIdOf('turtle:a1b2')).toBeNull()
  expect(personIdOf('turtle:member/a/b')).toBeNull()
  expect(personIdOf('turtle:member/')).toBeNull()
})

test('someone whose Profile the reader sees is shown with their picture and name, leading to the Profile', () => {
  const ada = person('Ada Lovelace', { picture: '/api/profile_pictures/ada-lovelace?v=1' })
  expect(badgeOf({ id: ada.id, name: ada.name, profile: ada })).toEqual({
    state: 'profile',
    name: 'Ada Lovelace',
    address: '/u/ada-lovelace/ada-lovelace',
    picture: '/api/profile_pictures/ada-lovelace?v=1',
    active: true,
  })
})

test('a deactivated person keeps their last name and their Profile stays reachable', () => {
  const ken = person('Ken', { active: false })
  expect(badgeOf({ id: ken.id, name: ken.name, profile: ken })).toMatchObject({ state: 'profile', name: 'Ken', active: false })
})

test('someone whose Profile the reader may not see is shown by name alone', () => {
  expect(badgeOf({ id: 'ken', name: 'Ken', profile: null })).toEqual({ state: 'name', name: 'Ken' })
})

test('a Mention of nobody known, or whose name could not be loaded, says so', () => {
  expect(badgeOf(null)).toEqual({ state: 'unknown' })
  expect(badgeOf(undefined)).toEqual({ state: 'failed' })
})

test('the picker offers the active people whose name or email matches, by name', () => {
  const people = [person('Ada Lovelace'), person('Émile Zola'), person('Ken', { active: false }), person('Grace Hopper')]

  expect(choices(people, '').map(p => p.name)).toEqual(['Ada Lovelace', 'Émile Zola', 'Grace Hopper'])
  expect(choices(people, 'lOVE').map(p => p.name)).toEqual(['Ada Lovelace'])
  expect(choices(people, 'emile').map(p => p.name)).toEqual(['Émile Zola'])
  expect(choices(people, 'grace-hopper@').map(p => p.name)).toEqual(['Grace Hopper'])
  expect(choices(people, 'ken')).toEqual([])
})

test('the picker shows a handful of people at most', () => {
  const people = Array.from({ length: 20 }, (_, i) => person(`Person ${i}`))
  expect(choices(people, 'person')).toHaveLength(8)
})
