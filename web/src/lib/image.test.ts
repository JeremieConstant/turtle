import { getSchema } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import { expect, test } from 'vitest'
import { Image } from './image'

// The server reads an image as an inline `image` element with `src` and
// `alt` (server/src/ydoc.rs); the editor has to write it that way.
test('an image stands in the text with its address and description', () => {
  const schema = getSchema([StarterKit, Image.configure({ attach: async () => null })])
  const image = schema.nodes.image
  expect(image.isInline).toBe(true)
  expect(image.isAtom).toBe(true)
  const node = image.create({ src: '/api/attachments/a1', alt: 'logo.png' })
  expect(node.attrs).toEqual({ src: '/api/attachments/a1', alt: 'logo.png' })
  expect(schema.nodes.paragraph.create(null, [schema.text('Logo: '), node]).childCount).toBe(2)
})
