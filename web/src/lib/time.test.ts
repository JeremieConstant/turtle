import { expect, test } from 'vitest'
import { relativeTime } from './time'

// Seconds since the Unix epoch, as Turtle reports them: 14 Nov 2023, 22:13 UTC.
const NOW = 1_700_000_000

test('a moment ago reads as just now', () => {
  expect(relativeTime(NOW, NOW)).toBe('just now')
  expect(relativeTime(NOW - 40, NOW)).toBe('just now')
})

test('within the hour it counts minutes, within the day hours', () => {
  expect(relativeTime(NOW - 60, NOW)).toBe('1 minute ago')
  expect(relativeTime(NOW - 4 * 60, NOW)).toBe('4 minutes ago')
  expect(relativeTime(NOW - 3 * 3600, NOW)).toBe('3 hours ago')
})

test('within the week it counts days', () => {
  expect(relativeTime(NOW - 24 * 3600, NOW)).toBe('yesterday')
  expect(relativeTime(NOW - 3 * 24 * 3600, NOW)).toBe('3 days ago')
})

test('anything older is a date', () => {
  // Midday of 15 Oct in UTC, so it is that day wherever the test runs.
  expect(relativeTime(NOW - 30 * 24 * 3600 - 10 * 3600, NOW)).toBe('15 Oct')
})
