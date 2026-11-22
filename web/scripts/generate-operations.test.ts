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

const restored = (properties: Record<string, unknown>) => ({
  title: 'Restored',
  type: 'object',
  properties,
  required: Object.keys(properties),
})

test('two different types of the same name are refused, naming both', () => {
  const versions = restored({ number: { type: 'integer' } })
  const trash = restored({ id: { type: 'string' } })
  expect(() =>
    generate([
      { name: 'restore_version', description: 'Restores a Version.', browser_only: false, input: { type: 'object' }, output: versions },
      { name: 'restore_from_trash', description: 'Restores from the Trash.', browser_only: false, input: { type: 'object' }, output: trash },
    ]),
  ).toThrow(/Restored.*number: number.*id: string/s)
})

test('the same type reached from several operations is generated once', () => {
  const area = { type: 'object', properties: { name: { type: 'string' } }, required: ['name'] }
  const generated = generate([
    { name: 'get_area', description: 'Gets an Area.', browser_only: false, input: { type: 'object' }, output: { title: 'Area', ...area } },
    {
      name: 'list_areas',
      description: 'Lists the Areas.',
      browser_only: false,
      input: { type: 'object' },
      output: { type: 'array', items: { $ref: '#/$defs/Area' }, $defs: { Area: area } },
    },
  ])
  expect(generated.match(/export type Area =/g)).toHaveLength(1)
})
