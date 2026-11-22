// The dark appearance is a second set of values for the colour tokens
// (issue #50). These tests read tokens.css as the browser would in each
// appearance and hold its colours to WCAG AA contrast in both.

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'

const TOKENS = readFileSync(fileURLToPath(new URL('../src/styles/tokens.css', import.meta.url)), 'utf8').replace(
  /\/\*[\s\S]*?\*\//g,
  '',
)

/** Light or dark: the two sets of values, not the person's choice between them. */
type Scheme = 'light' | 'dark'

/** Every colour token as written in the :root of tokens.css, name and value. */
function declaredColours(): [name: string, value: string][] {
  const root = TOKENS.match(/:root\s*\{([\s\S]*?)\n\}/)?.[1] ?? ''
  return [...root.matchAll(/(--color-[\w-]+)\s*:\s*([^;]+);/g)].map(([, name, value]) => [name, value.trim()])
}

/** The value of every colour token in one appearance. */
function colourTokens(appearance: Scheme): Map<string, string> {
  return new Map(
    declaredColours().map(([name, value]) => {
      const pair = value.match(/^light-dark\((.+)\)$/)
      if (!pair) return [name, value]
      const [light, dark] = splitTopLevel(pair[1])
      return [name, appearance === 'light' ? light : dark]
    }),
  )
}

/** Splits at the commas that are not inside parentheses. */
function splitTopLevel(text: string): string[] {
  const parts: string[] = []
  let depth = 0
  let start = 0
  for (let at = 0; at < text.length; at++) {
    if (text[at] === '(') depth++
    else if (text[at] === ')') depth--
    else if (text[at] === ',' && depth === 0) {
      parts.push(text.slice(start, at).trim())
      start = at + 1
    }
  }
  parts.push(text.slice(start).trim())
  return parts
}

type Rgba = [number, number, number, number]

function parse(colour: string): Rgba {
  const hex = colour.match(/^#([0-9a-f]{6})$/i)
  if (hex) {
    const n = parseInt(hex[1], 16)
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1]
  }
  const rgb = colour.match(/^rgb\((\d+) (\d+) (\d+)(?: \/ ([\d.]+))?\)$/)
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3]), rgb[4] === undefined ? 1 : Number(rgb[4])]
  throw new Error(`Not a colour this test reads: ${colour}`)
}

/** A colour that may be see-through, as it looks over an opaque one. */
function over([r, g, b, a]: Rgba, [br, bg, bb]: Rgba): Rgba {
  return [r * a + br * (1 - a), g * a + bg * (1 - a), b * a + bb * (1 - a), 1]
}

function luminance([r, g, b]: Rgba): number {
  const channel = (value: number) => {
    const c = value / 255
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

function contrast(front: Rgba, back: Rgba): number {
  const [l1, l2] = [luminance(front), luminance(back)].sort((a, b) => b - a)
  return (l1 + 0.05) / (l2 + 0.05)
}

/** The contrast of one token on another in an appearance; the background lies on the page's own. */
function ratio(appearance: Scheme, front: string, back: string): number {
  const tokens = colourTokens(appearance)
  const value = (name: string) => {
    const found = tokens.get(name)
    if (!found) throw new Error(`${name} is not a colour token`)
    return parse(found)
  }
  const page = value('--color-bg')
  const behind = over(value(back), page)
  return contrast(over(value(front), behind), behind)
}

const SCHEMES: Scheme[] = ['light', 'dark']
const PERSONS = ['--color-person-1', '--color-person-2', '--color-person-3', '--color-person-4', '--color-person-5', '--color-person-6']
/** Where text and controls sit. */
const BACKGROUNDS = ['--color-bg', '--color-side', '--color-surface', '--color-sunken']

/** Every pair WCAG AA asks for, with the ratio it needs: 4.5 for text, 3 for what tells a control or its state. */
const PAIRS: [front: string, back: string, needs: number][] = [
  ...['--color-text', '--color-text-2', '--color-muted', '--color-danger', '--color-agent'].flatMap(front =>
    BACKGROUNDS.map(back => [front, back, 4.5] as [string, string, number]),
  ),
  // A row under the pointer or the open one, as in the sidebar.
  ...['--color-text', '--color-text-2', '--color-muted'].flatMap(front =>
    ['--color-hover', '--color-selected'].map(back => [front, back, 4.5] as [string, string, number]),
  ),
  // The primary button is the text colour with the page's colour on it.
  ['--color-bg', '--color-text', 4.5],
  ['--color-bg', '--color-text-2', 4.5],
  ['--color-on-color', '--color-danger', 4.5],
  // A comparison of Versions marks added text in the focus colour.
  ['--color-focus', '--color-side', 4.5],
  // The focus ring, and the border that shows where a field or button is.
  // The thin lines between rows only divide, so WCAG asks nothing of them.
  ...['--color-focus', '--color-line-strong'].flatMap(front =>
    BACKGROUNDS.map(back => [front, back, 3] as [string, string, number]),
  ),
  // A person's cursor and mark stand out from the page, and their name is read on their colour.
  ...PERSONS.flatMap(person => [
    ...BACKGROUNDS.map(back => [person, back, 3] as [string, string, number]),
    ['--color-on-color', person, 4.5] as [string, string, number],
  ]),
  // An Agent's change: its name on the band it marks.
  ['--color-agent', '--color-agent-soft', 4.5],
]

test('every colour token has a value for the light and the dark appearance', () => {
  const single = declaredColours()
    .filter(([, value]) => !value.startsWith('light-dark('))
    .map(([name]) => name)
  expect(single).toEqual([])
  expect(colourTokens('dark').get('--color-bg')).not.toEqual(colourTokens('light').get('--color-bg'))
})

test('the contrast rule matches worked examples', () => {
  expect(contrast(parse('#000000'), parse('#ffffff'))).toBeCloseTo(21, 1)
  expect(contrast(parse('#767676'), parse('#ffffff'))).toBeCloseTo(4.54, 2)
  expect(over(parse('rgb(0 0 0 / 0.5)'), parse('#ffffff'))).toEqual([127.5, 127.5, 127.5, 1])
})

test.each(SCHEMES)('text, control borders and focus rings meet WCAG AA in the %s appearance', appearance => {
  const failing = PAIRS.filter(([front, back, needs]) => ratio(appearance, front, back) < needs).map(
    ([front, back, needs]) => `${front} on ${back}: ${ratio(appearance, front, back).toFixed(2)} < ${needs}`,
  )
  expect(failing).toEqual([])
})

test('the appearance can be chosen or left to the system, and the browser follows it', () => {
  // Scrollbars and form fields take their look from color-scheme.
  expect(TOKENS).toMatch(/:root\s*\{\s*color-scheme:\s*light dark;/)
  expect(TOKENS).toMatch(/:root\[data-appearance='light'\]\s*\{\s*color-scheme:\s*light;\s*\}/)
  expect(TOKENS).toMatch(/:root\[data-appearance='dark'\]\s*\{\s*color-scheme:\s*dark;\s*\}/)
})
