// ADR-0007: colours, spacing and type are custom properties defined in
// tokens.css and nowhere else. These tests hold every other stylesheet and
// every component's <style> to that.

import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'

const SOURCE = fileURLToPath(new URL('../src', import.meta.url))
const TOKENS = join(SOURCE, 'styles', 'tokens.css')

/** Every stylesheet and component under src, with the CSS it holds. */
function styles(): { file: string; css: string }[] {
  const files = readdirSync(SOURCE, { recursive: true, encoding: 'utf8' })
    .filter(file => file.endsWith('.css') || file.endsWith('.svelte'))
    .map(file => join(SOURCE, file))
    .filter(file => file !== TOKENS)
  return files.map(file => {
    const text = readFileSync(file, 'utf8')
    const css = file.endsWith('.css')
      ? text
      : [...text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map(match => match[1]).join('\n')
    return { file: relative(SOURCE, file), css: css.replace(/\/\*[\s\S]*?\*\//g, '') }
  })
}

/** A colour or a length written out instead of taken from a token. */
const LITERAL = /#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|oklch)\(|(?<![\w-])\d*\.?\d+(?:px|rem|em|vh|vw|ch)\b/gi

test('styles outside tokens.css use tokens instead of literal colours and lengths', () => {
  const literals = styles().flatMap(({ file, css }) =>
    [...css.matchAll(LITERAL)].map(match => `${file}: ${match[0]}`),
  )
  expect(literals).toEqual([])
})

test('every custom property a style uses is defined in tokens.css or set by its own component', () => {
  const tokens = new Set([...readFileSync(TOKENS, 'utf8').matchAll(/(--[\w-]+)\s*:/g)].map(match => match[1]))
  const undefinedUses = styles().flatMap(({ file, css }) => {
    // A component may set its own, as People does per person with style:--person.
    const own = readFileSync(join(SOURCE, file), 'utf8')
    return [...css.matchAll(/var\((--[\w-]+)/g)]
      .map(match => match[1])
      .filter(name => !tokens.has(name) && !new RegExp(`(?:style:)?${name}\\s*[:=]`).test(own))
      .map(name => `${file}: ${name}`)
  })
  expect(undefinedUses).toEqual([])
})
