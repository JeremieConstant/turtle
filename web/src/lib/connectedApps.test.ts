import { expect, test } from 'vitest'
import { access, byMember } from './connectedApps'
import type { TokenSummary } from './operations.gen'

const NOW = 1_700_000_000

function entry(changes: Partial<TokenSummary>): TokenSummary {
  return { id: 't', name: 'Claude', read_only: false, connected: true, areas: null, expires_at: null, last_used_at: null, created_at: NOW, ...changes }
}

test('an app with full access reads and writes in every Area of the Member', () => {
  expect(access(entry({}), NOW)).toBe('Read and write · All your Areas')
})

test('a read-only Token limited to Areas names them', () => {
  const areas = [{ id: 'a', name: 'Handbook' }, { id: 'b', name: 'Platform' }]
  expect(access(entry({ read_only: true, areas, connected: false }), NOW)).toBe('Read-only · Only Handbook, Platform')
})

test('a Token limited to Areas the Member can no longer read says so', () => {
  expect(access(entry({ areas: [] }), NOW)).toBe('Read and write · Only Areas you can no longer read')
})

test('a Token with an expiry says until when it works, or that it expired', () => {
  expect(access(entry({ expires_at: NOW + 3600 }), NOW)).toMatch(/^Read and write · All your Areas · Until /)
  expect(access(entry({ expires_at: NOW - 1 }), NOW)).toMatch(/^Read and write · All your Areas · Expired /)
})

test("a Member's app reaching every Area reaches all of the Member's, as a Workspace Admin reads it", () => {
  expect(access(entry({}), NOW, 'All their Areas')).toBe('Read and write · All their Areas')
})

test("every Member's apps are grouped under the Member, in the order given", () => {
  const grace = { id: 'member-grace', name: 'Grace' }
  const ken = { id: 'member-ken', name: 'Ken' }
  const phone = entry({ id: 'p', name: 'Phone' })
  const laptop = entry({ id: 'l', name: 'Laptop' })
  const backup = entry({ id: 'b', name: 'Backup' })
  const grouped = byMember([
    { member: grace, token: phone },
    { member: grace, token: laptop },
    { member: ken, token: backup },
  ])
  expect(grouped).toEqual([
    { member: grace, apps: [phone, laptop] },
    { member: ken, apps: [backup] },
  ])
  expect(byMember([])).toEqual([])
})
