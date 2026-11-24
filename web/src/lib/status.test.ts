import { getSchema } from '@tiptap/core'
import { expect, test, vi } from 'vitest'
import { contentExtensions } from './extensions'
import { STATUS_COLORS, statusColor, statusText } from './status'

// A Page link in the schema is followed through the router, which reads the
// address bar as it loads; these tests never follow one.
vi.mock('./router.svelte', () => ({ router: {} }))

const schema = getSchema(contentExtensions())

test('a Status comes in six named colours, grey when it names none Turtle knows', () => {
  expect(STATUS_COLORS).toEqual(['grey', 'blue', 'green', 'yellow', 'red', 'purple'])
  expect(statusColor('green')).toBe('green')
  expect(statusColor('orange')).toBe('grey')
  expect(statusColor(null)).toBe('grey')
})

test('a Status keeps its text as typed, on one line and without a `|`, as Markdown can hold it', () => {
  expect(statusText('on Track ')).toBe('on Track ')
  expect(statusText('a|b\nc')).toBe('a/b c')
})

test('a Status is one piece of running text, in table cells and headings too', () => {
  const status = schema.nodes.status
  expect(status.isInline).toBe(true)
  expect(status.isAtom).toBe(true)
  expect(status.create().attrs).toEqual({ text: '', color: 'grey' })
  const marker = status.create({ text: 'On track', color: 'green' })
  const { paragraph, heading, tableCell } = schema.nodes
  expect(() => heading.create({ level: 2 }, [schema.text('Plan '), marker]).check()).not.toThrow()
  expect(() => tableCell.create(null, [paragraph.create(null, [marker])]).check()).not.toThrow()
})
