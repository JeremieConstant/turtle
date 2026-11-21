import { expect, test } from 'vitest'
import { areaAddress, pageAddress, parseAddress } from './routes'

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

test('the setup and sign-in screens have their own addresses', () => {
  expect(parseAddress('/setup')).toEqual({ screen: 'setup' })
  expect(parseAddress('/sign-in')).toEqual({ screen: 'sign-in' })
})

test('an Area address carries the ID and a readable name, which is optional', () => {
  expect(parseAddress('/a/xyz789/engineering')).toEqual({ screen: 'area', id: 'xyz789' })
  expect(parseAddress('/a/xyz789')).toEqual({ screen: 'area', id: 'xyz789' })
  expect(parseAddress('/a/xyz789/')).toEqual({ screen: 'area', id: 'xyz789' })
})

test('an Area address is built like a Page address and leads back to the same Area', () => {
  expect(areaAddress('xyz789', 'Engineering & Ops')).toBe('/a/xyz789/engineering-ops')
  expect(areaAddress('xyz789', '')).toBe('/a/xyz789')
  expect(parseAddress(areaAddress('a/b c', 'Some Name'))).toEqual({ screen: 'area', id: 'a/b c' })
})

test('broken Area addresses are not found', () => {
  for (const address of ['/a', '/a/', '/a/xyz/name/more', '/a/%E0%A4%A/x']) {
    expect(parseAddress(address), address).toEqual({ screen: 'not-found' })
  }
})

test('a Page address with an outdated title still leads to the same Page', () => {
  const before = pageAddress('abc123', 'Release notes')
  const after = pageAddress('abc123', 'Changelog')
  expect(after).not.toBe(before)
  expect(parseAddress(before)).toEqual(parseAddress(after))
})

test('the Members screen has its own address', () => {
  expect(parseAddress('/settings/members')).toEqual({ screen: 'members' })
})

test('an invitation link and a password reset link carry their secret', () => {
  expect(parseAddress('/join/ab12')).toEqual({ screen: 'invitation', secret: 'ab12' })
  expect(parseAddress('/reset-password/cd34')).toEqual({ screen: 'password-reset', secret: 'cd34' })
})

test('a link without a secret, or with more after it, is not found', () => {
  for (const address of ['/join', '/join/', '/join/ab/cd', '/reset-password/', '/reset-password/ab/cd']) {
    expect(parseAddress(address), address).toEqual({ screen: 'not-found' })
  }
})

test('the Tokens screen has its own address under settings', () => {
  expect(parseAddress('/settings/tokens')).toEqual({ screen: 'tokens' })
})
