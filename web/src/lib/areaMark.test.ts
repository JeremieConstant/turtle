import { expect, test } from 'vitest'
import { AREA_COLORS, areaColor, areaInitial } from './areaMark'

test('an Area keeps its colour, and the colour is one of the palette', () => {
  const ids = ['08d56e1b8f78c9e6e8ac7f7bf7718d88', 'a', 'b', 'area-engineering', '']
  for (const id of ids) {
    expect(areaColor(id)).toBe(areaColor(id))
    expect(areaColor(id)).toBeGreaterThanOrEqual(1)
    expect(areaColor(id)).toBeLessThanOrEqual(AREA_COLORS)
  }
})

test('different Areas spread over the palette', () => {
  const used = new Set(Array.from({ length: 60 }, (_, index) => areaColor(`area-${index}`)))
  expect(used.size).toBe(AREA_COLORS)
})

test('the mark shows the first letter or digit of the name as a capital', () => {
  expect(areaInitial('engineering')).toBe('E')
  expect(areaInitial('  Acme Engineering')).toBe('A')
  expect(areaInitial('🚀 launch')).toBe('L')
  expect(areaInitial('2026 plans')).toBe('2')
  expect(areaInitial('ödland')).toBe('Ö')
  expect(areaInitial('—')).toBe('·')
})
