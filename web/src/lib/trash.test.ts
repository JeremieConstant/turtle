import { expect, test } from 'vitest'
import { daysLeft, deletedBy, endsSoon, whereItWas } from './trash'

test('the days left are counted down to the last day', () => {
  expect(daysLeft(30)).toBe('30 days left')
  expect(daysLeft(2)).toBe('2 days left')
  expect(daysLeft(1)).toBe('Last day')
  expect(daysLeft(0)).toBe('Last day')
})

test('three days or fewer are marked', () => {
  expect([4, 3, 1, 0].map(endsSoon)).toEqual([false, true, true, true])
})

test('a row says where its Entry was', () => {
  expect(whereItWas({ was_beneath: [], restores_to_top: false }, 'Engineering')).toBe('Was at the top of Engineering')
  expect(whereItWas({ was_beneath: ['Guides', 'Setup'], restores_to_top: false }, 'Engineering')).toBe('Was in Guides / Setup')
})

test('a row whose parent is gone says that it will be restored to the top of the Area', () => {
  expect(whereItWas({ was_beneath: ['Guides'], restores_to_top: true }, 'Engineering')).toBe(
    'Was in Guides, which is in the Trash too. Restores to the top of Engineering.',
  )
})

test('a row names who deleted it, also when they are gone', () => {
  expect(deletedBy({ kind: 'member', id: 'm1', name: 'Grace' })).toBe('Deleted by Grace')
  expect(deletedBy({ kind: 'agent', id: 'a1', name: 'Reviewer' })).toBe('Deleted by Reviewer (Agent)')
  expect(deletedBy(null)).toBe('Deleted by someone no longer here')
})
