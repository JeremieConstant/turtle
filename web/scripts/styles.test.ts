// ADR-0007: colours, distances, type, radii and motion durations are custom
// properties defined in tokens.css and nowhere else. These tests hold every other stylesheet and
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
    // A media query cannot read a custom property, so the width a layout changes at is written out there.
    return { file: relative(SOURCE, file), css: css.replace(/\/\*[\s\S]*?\*\//g, '').replace(/@media[^{]*/g, '@media ') }
  })
}

/** A colour, a length or a duration written out instead of taken from a token. */
const LITERAL =
  /#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|oklch)\(|(?<![\w-])\d*\.?\d+(?:px|rem|em|vh|vw|dvh|ch|ms|s)\b/gi

test('the rule finds literal colours, lengths and durations', () => {
  const css = 'a { color: #1a1a19; inset: 0; width: 280px; height: 100dvh; transition: opacity 140ms, top .2s; flex: 1 1 0; }'
  expect(css.match(LITERAL)).toEqual(['#1a1a19', '280px', '100dvh', '140ms', '.2s'])
})

test('styles outside tokens.css use tokens instead of literal colours, lengths and durations', () => {
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

test('reduced motion sets every duration to zero', () => {
  const tokens = readFileSync(TOKENS, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')
  const reduced = tokens.match(/@media \(prefers-reduced-motion: reduce\) \{\s*:root \{([\s\S]*?)\}/)?.[1] ?? ''
  const durations = [...new Set([...tokens.matchAll(/(--dur-[\w-]+)\s*:/g)].map(match => match[1]))]
  expect(durations.length).toBeGreaterThan(0)
  const stillMoving = durations.filter(name => !new RegExp(`${name}\\s*:\\s*0ms\\s*;`).test(reduced))
  expect(stillMoving).toEqual([])
})

test('every duration in a style is a duration token', () => {
  // A transition or animation that names no --dur token would keep moving under reduced motion.
  const untimed = styles().flatMap(({ file, css }) =>
    [...css.matchAll(/(?<![\w-])(transition|animation)\s*:([^;}]+)/g)]
      .filter(match => !/var\(--dur-|\bnone\b/.test(match[2]))
      .map(match => `${file}: ${match[0].trim()}`),
  )
  expect(untimed).toEqual([])
})
