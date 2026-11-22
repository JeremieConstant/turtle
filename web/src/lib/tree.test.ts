import { expect, test } from 'vitest'
import type { Entry } from './operations.gen'
import { above, foldable, visibleRows } from './tree'

const page = (id: string, children: Entry[] = []): Entry => ({ id, kind: 'page', title: id, children })
const folder = (id: string, children: Entry[] = []): Entry => ({ id, kind: 'folder', title: id, children })

// Runbooks
//   Deploying
//     Rollback
//   Empty
// Glossary
const tree = [folder('runbooks', [page('deploying', [page('rollback')]), folder('empty')]), page('glossary')]

const shown = (collapsed: string[]) =>
  visibleRows(tree, new Set(collapsed)).map(row => `${row.depth} ${row.entry.id}${row.open ? '' : ' (folded)'}`)

test('with nothing folded, every Entry is a row, indented by how deep it sits', () => {
  expect(shown([])).toEqual(['0 runbooks', '1 deploying', '2 rollback', '1 empty', '0 glossary'])
})

test('a folded Entry hides everything beneath it and says it is folded', () => {
  expect(shown(['runbooks'])).toEqual(['0 runbooks (folded)', '0 glossary'])
})

test('what is folded beneath a folded Entry is still folded when it is opened again', () => {
  expect(shown(['deploying'])).toEqual(['0 runbooks', '1 deploying (folded)', '1 empty', '0 glossary'])
})

test('only Entries with something beneath them can fold', () => {
  expect(foldable(tree)).toEqual(['runbooks', 'deploying'])
})

test('the Entries above one are named from the top down', () => {
  expect(above(tree, 'rollback')).toEqual(['runbooks', 'deploying'])
  expect(above(tree, 'glossary')).toEqual([])
})

test('an Entry that is not in the tree has nothing above it', () => {
  expect(above(tree, 'elsewhere')).toEqual([])
})
