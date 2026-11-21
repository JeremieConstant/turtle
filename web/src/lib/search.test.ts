import { expect, test } from 'vitest'
import { latestOnly, place } from './search'

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
