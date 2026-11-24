import { getSchema } from '@tiptap/core'
import { expect, test, vi } from 'vitest'
import { placeholderText } from './directive'
import { contentExtensions } from './extensions'

// A Page link in the schema is followed through the router, which reads the
// address bar as it loads; these tests never follow one.
vi.mock('./router.svelte', () => ({ router: {} }))

const schema = getSchema(contentExtensions())

test('a placeholder names the directive Turtle does not know', () => {
  expect(placeholderText(':acme.demo[hi]{x=1}')).toBe('Unknown element: acme.demo')
  expect(placeholderText(':::acme.board{x=1}\n- a\n:::')).toBe('Unknown element: acme.board')
  expect(placeholderText('::::chart.\n::::')).toBe('Unknown element: chart')
})

test('an unknown directive is one locked piece, inline or as a block', () => {
  const inline = schema.nodes.unknownDirective
  expect(inline.isInline).toBe(true)
  expect(inline.isAtom).toBe(true)
  expect(inline.spec.draggable).toBe(true)
  const block = schema.nodes.unknownDirectiveBlock
  expect(block.isBlock).toBe(true)
  expect(block.isAtom).toBe(true)
  expect(block.spec.draggable).toBe(true)
  expect(block.create({ source: ':::acme.x\n:::' }).attrs).toEqual({ source: ':::acme.x\n:::' })
})

test('a block directive stands only at the top of a Page, as Markdown can hold it', () => {
  const block = schema.nodes.unknownDirectiveBlock.create({ source: ':::acme.x\n:::' })
  expect(() => schema.nodes.doc.create(null, [block]).check()).not.toThrow()
  expect(() => schema.nodes.blockquote.create(null, [block]).check()).toThrow()
  expect(() => schema.nodes.listItem.create(null, [schema.nodes.paragraph.create(), block]).check()).toThrow()
})
