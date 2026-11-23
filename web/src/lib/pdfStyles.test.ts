import { expect, test } from 'vitest'
import type { PdfStyle } from './operations.gen'
import { defaultPdfStyleId, newPdfStyleValues, valuesOf } from './pdfStyles'

const standard: PdfStyle = {
  id: 'standard',
  is_default: true,
  name: 'Standard',
  text_color: '#23252b',
  heading_color: '#17181c',
  accent_color: '#2f5f8a',
  body_font: 'Inter',
  heading_font: 'Inter',
  code_font: 'JetBrains Mono',
  body_size: 9.6,
  h1_size: 26,
  h2_size: 13.5,
  h3_size: 11,
  cover: true,
  toc: true,
  toc_depth: 2,
  footer_text: null,
  language: 'en',
}

const handout: PdfStyle = { ...standard, id: 'h1', is_default: false, name: 'Handout', body_font: 'Source Serif 4', language: 'de' }

test('the default PDF Style is the one an export starts with', () => {
  expect(defaultPdfStyleId([handout, standard])).toBe('standard')
  expect(defaultPdfStyleId([{ ...handout, is_default: true }, { ...standard, is_default: false }])).toBe('h1')
  expect(defaultPdfStyleId([])).toBeUndefined()
})

test('a new PDF Style starts as a nameless copy of the default one', () => {
  const { name, ...rest } = newPdfStyleValues([handout, standard])
  expect(name).toBe('')
  expect(rest).toEqual({
    text_color: '#23252b',
    heading_color: '#17181c',
    accent_color: '#2f5f8a',
    body_font: 'Inter',
    heading_font: 'Inter',
    code_font: 'JetBrains Mono',
    body_size: 9.6,
    h1_size: 26,
    h2_size: 13.5,
    h3_size: 11,
    cover: true,
    toc: true,
    toc_depth: 2,
    footer_text: '',
    language: 'en',
  })
})

test('a PDF Style is edited by its values alone', () => {
  const values = valuesOf(handout)
  expect(values).not.toHaveProperty('id')
  expect(values).not.toHaveProperty('is_default')
  expect(values).toMatchObject({ name: 'Handout', body_font: 'Source Serif 4', language: 'de', footer_text: '' })
})
