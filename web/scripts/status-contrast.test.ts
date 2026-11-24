// The colours of a Status are design tokens per theme (web/src/styles/tokens.css),
// each a fill and the text on it. They have to stay readable in both.

import { readFileSync } from 'node:fs'
import { expect, test } from 'vitest'
import { STATUS_COLORS } from '../src/lib/status'

const tokens = readFileSync(new URL('../src/styles/tokens.css', import.meta.url), 'utf8')

/** The relative luminance of a `#rrggbb` colour (WCAG 2). */
function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map(at => {
    const c = parseInt(hex.slice(at, at + 2), 16) / 255
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  })
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (light + 0.05) / (dark + 0.05)
}

test('every Status colour meets WCAG AA text contrast against its fill, in light and in dark mode', () => {
  const shades = (name: string) => {
    const found = new RegExp(`--${name}:\\s*light-dark\\((#[0-9a-f]{6}),\\s*(#[0-9a-f]{6})\\)`, 'i').exec(tokens)
    if (!found) throw new Error(`--${name} is not a pair of light and dark shades`)
    return { light: found[1], dark: found[2] }
  }
  for (const color of STATUS_COLORS) {
    const fill = shades(`color-status-${color}`)
    const text = shades(`color-status-${color}-text`)
    expect(contrast(text.light, fill.light), `${color} in light mode`).toBeGreaterThanOrEqual(4.5)
    expect(contrast(text.dark, fill.dark), `${color} in dark mode`).toBeGreaterThanOrEqual(4.5)
  }
})
