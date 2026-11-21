import { expect, test } from 'vitest'
import { generate, typeOf } from './generate-operations'

test('simple types, unions, enums and constants become their TypeScript form', () => {
  expect(typeOf({ type: 'string' })).toBe('string')
  expect(typeOf({ type: 'integer' })).toBe('number')
  expect(typeOf({ type: ['string', 'null'] })).toBe('string | null')
  expect(typeOf({ anyOf: [{ $ref: '#/$defs/Member' }, { type: 'null' }] })).toBe('Member | null')
  expect(typeOf({ enum: ['read', 'write'] })).toBe('"read" | "write"')
  expect(typeOf({ const: 'page' })).toBe('"page"')
  expect(typeOf({ type: 'array', items: { anyOf: [{ type: 'string' }, { type: 'null' }] } })).toBe(
    '(string | null)[]',
  )
})

test('an object lists its properties, optional ones marked, with their descriptions', () => {
  const schema = {
    type: 'object',
    properties: {
      id: { type: 'string' },
      parent_id: { type: ['string', 'null'], description: 'Where it goes.' },
    },
    required: ['id'],
  }
  expect(typeOf(schema)).toBe('{\n  id: string\n  /** Where it goes. */\n  parent_id?: string | null\n}')
})

test('an object without properties is an empty record', () => {
  expect(typeOf({ type: 'object' })).toBe('Record<string, never>')
})

test('a titled output becomes a named type, and a type that contains itself refers to its name', () => {
  const output = {
    $schema: 'https://json-schema.org/draft/2020-12/schema',
    title: 'Entry',
    description: 'A Page or Folder.',
    type: 'object',
    properties: {
      children: { type: 'array', items: { $ref: '#' } },
      kind: { $ref: '#/$defs/Kind' },
    },
    required: ['children', 'kind'],
    $defs: { Kind: { type: 'string', enum: ['page', 'folder'] } },
  }
  const generated = generate([
    { name: 'create_page', description: 'Creates a Page.', browser_only: false, input: { type: 'object' }, output },
  ])

  expect(generated).toContain('export type Entry = {\n  children: Entry[]\n  kind: Kind\n}')
  expect(generated).toContain('export type Kind = "page" | "folder"')
  expect(generated).toContain('    output: Entry')
  expect(generated).not.toContain('#')
})

test('an untitled output stays inline', () => {
  const generated = generate([
    { name: 'sign_out', description: 'Signs out.', browser_only: true, input: { type: 'object' }, output: { type: 'null' } },
  ])
  expect(generated).toContain('    output: null')
})
