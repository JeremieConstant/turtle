import { expect, test } from 'vitest'
import { pageAddress, parseAddress } from './routes'

test('a Page address carries the ID and a readable title', () => {
  expect(parseAddress('/p/abc123/release-notes')).toEqual({ screen: 'page', id: 'abc123' })
})

test('a Page address works without the title part', () => {
  expect(parseAddress('/p/abc123')).toEqual({ screen: 'page', id: 'abc123' })
  expect(parseAddress('/p/abc123/')).toEqual({ screen: 'page', id: 'abc123' })
})

test('a Page ID is decoded from the address', () => {
  expect(parseAddress('/p/a%20b/x')).toEqual({ screen: 'page', id: 'a b' })
})

test('the root address is the home screen', () => {
  expect(parseAddress('/')).toEqual({ screen: 'home' })
})

test('unknown addresses are not found', () => {
  for (const address of ['/nowhere', '/p', '/p/', '/p/abc/title/more', '/p/%E0%A4%A/x', '/pages/abc']) {
    expect(parseAddress(address), address).toEqual({ screen: 'not-found' })
  }
})

test('a Page address is built from the ID and the title made readable', () => {
  expect(pageAddress('abc123', 'Release Notes: Q3 / Draft')).toBe('/p/abc123/release-notes-q3-draft')
  expect(pageAddress('abc123', 'Überblick')).toBe('/p/abc123/%C3%BCberblick')
  expect(pageAddress('a b', 'x')).toBe('/p/a%20b/x')
})

test('a title with nothing readable leaves the title part out', () => {
  expect(pageAddress('abc123', '')).toBe('/p/abc123')
  expect(pageAddress('abc123', '?!')).toBe('/p/abc123')
})

test('a built Page address leads back to the same Page', () => {
  expect(parseAddress(pageAddress('a/b c', 'Some Title'))).toEqual({ screen: 'page', id: 'a/b c' })
})
