import { expect, test } from 'vitest'
import { initials, standing } from './profile'

test('initials are the first letters of the first and last word of the name', () => {
  expect(initials('Ada Lovelace')).toBe('AL')
  expect(initials('Grace Brewster Murray Hopper')).toBe('GH')
})

test('a single name gives one initial', () => {
  expect(initials('Ada')).toBe('A')
})

test('initials are upper case and ignore extra spaces', () => {
  expect(initials('  ada   lovelace ')).toBe('AL')
})

test('initials take whole letters, not halves of one', () => {
  expect(initials('Émile Zola')).toBe('ÉZ')
  expect(initials('𝒜da 𝒵ola')).toBe('𝒜𝒵')
})

test('a name without letters still gives a mark', () => {
  expect(initials('')).toBe('?')
  expect(initials('   ')).toBe('?')
})

test('a Guest and a Workspace Admin are named as such, anyone else by nothing', () => {
  expect(standing({ guest: true, workspace_admin: false })).toBe('Guest')
  expect(standing({ guest: false, workspace_admin: true })).toBe('Workspace Admin')
  expect(standing({ guest: false, workspace_admin: false })).toBeNull()
})
