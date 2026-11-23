import { getSchema } from '@tiptap/core'
import { EditorState, TextSelection } from '@tiptap/pm/state'
import { CellSelection } from '@tiptap/pm/tables'
import { expect, test, vi } from 'vitest'
import { contentExtensions } from './extensions'
import { allowedTableEdits, clampSize, TABLE_SIZE } from './table'

// A Page link is followed through the router, which reads the address bar as it loads.
vi.mock('./router.svelte', () => ({ router: {} }))

const schema = getSchema(contentExtensions())
const paragraph = (text = '') => ({ type: 'paragraph', content: text ? [{ type: 'text', text }] : [] })
const row = (type: string, ...texts: string[]) => ({ type: 'tableRow', content: texts.map(t => ({ type, content: [paragraph(t)] })) })

// | a | b |
// | c | d |
// | e | f |
const doc = schema.nodeFromJSON({
  type: 'doc',
  content: [{ type: 'table', content: [row('tableHeader', 'a', 'b'), row('tableCell', 'c', 'd'), row('tableCell', 'e', 'f')] }, paragraph('after')],
})

/** The position just inside the text of the cell that says `text`. */
function inside(text: string): number {
  let at = -1
  doc.descendants((node, pos) => {
    if (node.isText && node.text === text) at = pos
  })
  return at
}

const cursorIn = (text: string) => EditorState.create({ doc, selection: TextSelection.create(doc, inside(text)) })
const cellsFrom = (from: string, to: string) => {
  const cell = (text: string) => doc.resolve(inside(text) - 2)
  return EditorState.create({ doc, selection: new CellSelection(cell(from), cell(to)) })
}

test('a cell holds a single paragraph and no other block', () => {
  const cell = (content: object[]) =>
    schema.nodeFromJSON({ type: 'doc', content: [{ type: 'table', content: [{ type: 'tableRow', content: [{ type: 'tableHeader', content }] }] }] })
  expect(() => cell([paragraph('fine')]).check()).not.toThrow()
  expect(() => cell([paragraph('one'), paragraph('two')]).check()).toThrow()
  expect(() => cell([{ type: 'heading', attrs: { level: 1 }, content: [{ type: 'text', text: 'x' }] }]).check()).toThrow()
  expect(() => cell([{ type: 'bulletList', content: [{ type: 'listItem', content: [paragraph('x')] }] }]).check()).toThrow()
})

test('in the header row, rows are only added below and the row is not deleted', () => {
  expect([...allowedTableEdits(cursorIn('a'))].sort()).toEqual(['column-after', 'column-before', 'delete-column', 'delete-table', 'row-below'])
})

test('in any other row, every edit applies', () => {
  expect([...allowedTableEdits(cursorIn('d'))].sort()).toEqual([
    'column-after',
    'column-before',
    'delete-column',
    'delete-row',
    'delete-table',
    'row-above',
    'row-below',
  ])
})

test('a selection that reaches into the header row cannot delete its rows, and one across every column cannot delete them', () => {
  const allowed = allowedTableEdits(cellsFrom('a', 'd'))
  expect(allowed.has('delete-row')).toBe(false)
  expect(allowed.has('delete-column')).toBe(false)
  expect(allowedTableEdits(cellsFrom('c', 'e')).has('delete-column')).toBe(true)
})

test('outside a table no edit applies', () => {
  expect(allowedTableEdits(cursorIn('after')).size).toBe(0)
})

test('the size asked for in the dialog stays within what it allows', () => {
  expect(clampSize(4, TABLE_SIZE.rows)).toBe(4)
  expect(clampSize(0, TABLE_SIZE.rows)).toBe(1)
  expect(clampSize(500, TABLE_SIZE.columns)).toBe(20)
  expect(clampSize(2.6, TABLE_SIZE.columns)).toBe(3)
  expect(clampSize(Number.NaN, TABLE_SIZE.rows)).toBe(1)
})
