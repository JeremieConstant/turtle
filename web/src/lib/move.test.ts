import { expect, test } from 'vitest'
import { dropPlace, newPlace, placesBeneath, writableAreas } from './move'
import type { Area, Entry } from './operations.gen'

const page = (id: string, children: Entry[] = []): Entry => ({ id, kind: 'page', title: id, children })
const folder = (id: string, children: Entry[] = []): Entry => ({ id, kind: 'folder', title: id, children })

// runbooks
//   deploying
//     rollback
//   empty
// glossary
// faq
const tree = [folder('runbooks', [page('deploying', [page('rollback')]), folder('empty')]), page('glossary'), page('faq')]

test('dropped before a row, an Entry goes before it among that row’s siblings', () => {
  expect(dropPlace(tree, 'faq', 'glossary', 'before')).toEqual({ parent_id: undefined, before_id: 'glossary' })
  expect(dropPlace(tree, 'faq', 'empty', 'before')).toEqual({ parent_id: 'runbooks', before_id: 'empty' })
})

test('dropped after a row, an Entry goes before the next sibling, or last when there is none', () => {
  expect(dropPlace(tree, 'faq', 'runbooks', 'after')).toEqual({ parent_id: undefined, before_id: 'glossary' })
  expect(dropPlace(tree, 'glossary', 'empty', 'after')).toEqual({ parent_id: 'runbooks', before_id: undefined })
  // Right after the sibling before it, an Entry stays where it is.
  expect(dropPlace(tree, 'glossary', 'runbooks', 'after')).toBeNull()
})

test('dropped onto a row, an Entry goes last beneath it', () => {
  expect(dropPlace(tree, 'glossary', 'deploying', 'inside')).toEqual({ parent_id: 'deploying', before_id: undefined })
})

test('an Entry cannot be dropped onto itself or anything beneath it', () => {
  expect(dropPlace(tree, 'runbooks', 'runbooks', 'inside')).toBeNull()
  expect(dropPlace(tree, 'runbooks', 'rollback', 'before')).toBeNull()
  expect(dropPlace(tree, 'runbooks', 'empty', 'after')).toBeNull()
})

test('a drop that leaves an Entry where it is moves nothing', () => {
  expect(dropPlace(tree, 'glossary', 'faq', 'before')).toBeNull()
  expect(dropPlace(tree, 'faq', 'glossary', 'after')).toBeNull()
  expect(dropPlace(tree, 'faq', 'faq', 'before')).toBeNull()
  expect(dropPlace(tree, 'empty', 'runbooks', 'inside')).toBeNull()
})

test('the places to move an Entry beneath leave out the Entry and everything beneath it', () => {
  const places = placesBeneath(tree, 'deploying').map(place => `${place.depth} ${place.entry.id}`)
  expect(places).toEqual(['0 runbooks', '1 empty', '0 glossary', '0 faq'])
})

const area = (id: string, role: Area['role'], personal = false): Area => ({ id, name: id, role, personal })

test('an Entry can be moved only to Areas where the person holds Write', () => {
  const areas = [area('personal', 'admin', true), area('engineering', 'write'), area('board', 'read')]
  expect(writableAreas(areas).map(each => each.id)).toEqual(['personal', 'engineering'])
})

test('after a move, its new place is named by its Area and what it is beneath', () => {
  const sales = area('Sales', 'write')
  const guides = { id: 'g', kind: 'folder' as const, title: 'Guides' }
  const handbook = { id: 'h', kind: 'page' as const, title: 'Handbook' }
  expect(newPlace({ area: sales, path: [guides, handbook] })).toBe('Sales / Guides / Handbook')
  expect(newPlace({ area: sales, path: [] })).toBe('the top of Sales')
})
