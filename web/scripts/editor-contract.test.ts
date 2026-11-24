// The server reads and writes Page content in the collaborative document by
// the editor's element and attribute names (server/src/ydoc.rs). This test
// and the server's `the_editor_contract` tests share fixtures in
// server/tests/fixtures, so renaming a node or attribute on either side
// fails a test instead of quietly dropping content:
//
// - from-editor.bin is this content as the editor stores it; the server reads
//   it as editor-contract.md.
// - from-server.bin is editor-contract.md as the server writes it; the
//   editor's schema reads it as this content.
//
// After a deliberate change, write both fixtures anew with
// `UPDATE_CONTRACT=1 npm --prefix web test` and
// `UPDATE_CONTRACT=1 cargo test --bin turtle the_editor_contract` (from server/).

import { getSchema } from '@tiptap/core'
import { prosemirrorJSONToYXmlFragment, yXmlFragmentToProsemirrorJSON } from '@tiptap/y-tiptap'
import { readFileSync, writeFileSync } from 'node:fs'
import { expect, test, vi } from 'vitest'
import * as Y from 'yjs'
import { contentExtensions } from '../src/lib/extensions'

// A Page link and a Mention are followed through the router, which reads the address bar as
// it loads; the schema this test needs never follows one.
vi.mock('../src/lib/router.svelte', () => ({ router: {} }))

const FIXTURES = new URL('../../server/tests/fixtures/', import.meta.url)
const UPDATE = process.env.UPDATE_CONTRACT === '1'
const FRAGMENT = 'default'

const text = (text: string, ...marks: object[]) => ({ type: 'text', text, ...(marks.length ? { marks } : {}) })
const paragraph = (...content: object[]) => ({ type: 'paragraph', content })
const item = (...content: object[]) => ({ type: 'listItem', content })
const task = (checked: boolean, ...content: object[]) => ({ type: 'taskItem', attrs: { checked }, content })
const callout = (kind: string, words: string) => ({ type: 'callout', attrs: { kind }, content: [paragraph(text(words))] })
const row = (cell: string, ...cells: object[][]) => ({ type: 'tableRow', content: cells.map(content => ({ type: cell, content: [paragraph(...content)] })) })

/** One of everything a Page can hold; editor-contract.md is the same content in Markdown. */
const CONTENT = {
  type: 'doc',
  content: [
    { type: 'heading', attrs: { level: 1 }, content: [text('Release')] },
    paragraph(
      text('Some '),
      text('bold', { type: 'bold' }),
      text(', '),
      text('italic', { type: 'italic' }),
      text(', '),
      text('struck', { type: 'strike' }),
      text(', '),
      text('code', { type: 'code' }),
      text(' and a '),
      text('link', { type: 'link', attrs: { href: 'https://example.org' } }),
      text('.'),
      { type: 'hardBreak' },
      text('Next line, and '),
      { type: 'pageLink', attrs: { pageId: 'a1b2' } },
      text('.'),
    ),
    paragraph(text('With '), { type: 'image', attrs: { src: 'logo.png', alt: 'Logo' } }, text(' inline.')),
    paragraph(text('A '), { type: 'unknownDirective', attrs: { source: ':acme.demo[hi]{x=1}' } }, text(' kept.')),
    paragraph(text('Now '), { type: 'status', attrs: { text: 'On track', color: 'green' } }, text('.')),
    { type: 'unknownDirectiveBlock', attrs: { source: ':::acme.board{x=1}\n- a\n:::' } },
    {
      type: 'bulletList',
      content: [
        item(paragraph(text('one'))),
        item(paragraph(text('two')), { type: 'orderedList', attrs: { start: 1 }, content: [item(paragraph(text('nested')))] }),
      ],
    },
    { type: 'orderedList', attrs: { start: 3 }, content: [item(paragraph(text('three'))), item(paragraph(text('four')))] },
    {
      type: 'taskList',
      content: [
        task(false, paragraph(text('open'))),
        task(true, paragraph(text('done')), { type: 'taskList', content: [task(false, paragraph(text('nested task')))] }),
      ],
    },
    { type: 'blockquote', content: [paragraph(text('quoted'))] },
    callout('note', 'A note.'),
    callout('tip', 'A tip.'),
    callout('warning', 'A warning.'),
    callout('danger', 'Danger.'),
    {
      type: 'table',
      content: [
        row('tableHeader', [text('Name')], [text('Role', { type: 'bold' })]),
        row('tableCell', [text('a | b')], [{ type: 'mention', attrs: { personId: 'm1' } }]),
      ],
    },
    { type: 'codeBlock', attrs: { language: 'rust' }, content: [text('fn main() {}')] },
    { type: 'horizontalRule' },
  ],
}

const schema = getSchema(contentExtensions())

test('the content is valid for the editor', () => {
  expect(() => schema.nodeFromJSON(CONTENT).check()).not.toThrow()
})

test('the content as the editor stores it is the fixture the server reads', () => {
  const doc = new Y.Doc()
  // A fixed client makes the stored bytes the same on every run.
  doc.clientID = 1
  prosemirrorJSONToYXmlFragment(schema, CONTENT, doc.getXmlFragment(FRAGMENT))
  const stored = Y.encodeStateAsUpdate(doc)
  const fixture = new URL('from-editor.bin', FIXTURES)
  if (UPDATE) writeFileSync(fixture, stored)
  expect(
    Buffer.compare(Buffer.from(stored), readFileSync(fixture)),
    'from-editor.bin is out of date; write it anew as the comment at the top of this file says',
  ).toBe(0)
})

test('what the server writes reads as the same content in the editor', () => {
  const doc = new Y.Doc()
  Y.applyUpdate(doc, readFileSync(new URL('from-server.bin', FIXTURES)))
  // Reading through the schema fills in default attributes and fails on any
  // element or attribute the editor does not know.
  const read = schema.nodeFromJSON(yXmlFragmentToProsemirrorJSON(doc.getXmlFragment(FRAGMENT)))
  read.check()
  expect(read.toJSON()).toEqual(schema.nodeFromJSON(CONTENT).toJSON())
})
