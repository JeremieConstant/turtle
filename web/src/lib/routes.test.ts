import { expect, test } from 'vitest'
import { areaAddress, inShell, pageAddress, parseAddress, searchAddress, tagSearchAddress, trashAddress } from './routes'

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

test('Connected apps has its own address under settings', () => {
  expect(parseAddress('/settings/connected-apps')).toEqual({ screen: 'connected-apps' })
})

test('the Tokens screen is gone, and its old address leads nowhere', () => {
  expect(parseAddress('/settings/tokens')).toEqual({ screen: 'not-found' })
})

test('the Agents screen has its own address under settings', () => {
  expect(parseAddress('/settings/agents')).toEqual({ screen: 'agents' })
})

test('the Connect AI page has its own address', () => {
  expect(parseAddress('/connect-ai')).toEqual({ screen: 'connect-ai' })
})

test('the screens of a signed-in person sit in the shell', () => {
  for (const address of ['/', '/connect-ai', '/a/abc/platform', '/p/abc/notes', '/settings/members', '/settings/connected-apps', '/settings/agents', '/nowhere']) {
    expect(inShell(parseAddress(address)), address).toBe(true)
  }
})

test('setup, signing in and the links someone was given stay outside the shell', () => {
  for (const address of ['/setup', '/sign-in', '/join/s3cret', '/reset-password/s3cret']) {
    expect(inShell(parseAddress(address)), address).toBe(false)
  }
})

test('the search screen carries its query in the address', () => {
  expect(parseAddress('/search', '?q=rollback')).toEqual({ screen: 'search', query: 'rollback' })
  expect(parseAddress('/search', '?q=release%20notes')).toEqual({ screen: 'search', query: 'release notes' })
  expect(parseAddress('/search', '?q=a+b')).toEqual({ screen: 'search', query: 'a b' })
})

test('the search screen without a query has an empty one', () => {
  expect(parseAddress('/search', '')).toEqual({ screen: 'search', query: '' })
  expect(parseAddress('/search')).toEqual({ screen: 'search', query: '' })
})

test('a search address is built from the query and leads back to it', () => {
  expect(searchAddress('rollback')).toBe('/search?q=rollback')
  expect(searchAddress('größe & co?')).toBe('/search?q=gr%C3%B6%C3%9Fe%20%26%20co%3F')
  expect(searchAddress('')).toBe('/search')
  const [pathname, query] = searchAddress('a+b = c#d').split('?')
  expect(parseAddress(pathname, `?${query}`)).toEqual({ screen: 'search', query: 'a+b = c#d' })
})

test('a Tag leads to the search for it', () => {
  expect(tagSearchAddress('release-notes')).toBe('/search?q=%23release-notes')
  expect(parseAddress('/search', '?q=%23release-notes')).toEqual({ screen: 'search', query: '#release-notes' })
})

test('the search screen sits in the shell', () => {
  expect(inShell(parseAddress('/search', '?q=x'))).toBe(true)
})

test('the Trash of an Area has an address of its own, after the Area’s', () => {
  expect(trashAddress('abc123', 'Engineering')).toBe('/a/abc123/engineering/trash')
  expect(parseAddress('/a/abc123/engineering/trash')).toEqual({ screen: 'trash', areaId: 'abc123' })
  expect(parseAddress(trashAddress('a/b c', ''))).toEqual({ screen: 'trash', areaId: 'a/b c' })
})

test('the deleted Areas are among the settings', () => {
  expect(parseAddress('/settings/deleted-areas')).toEqual({ screen: 'deleted-areas' })
})

test('the consent page keeps the request of the app that asks to connect', () => {
  const query = '?client_id=c1&redirect_uri=https%3A%2F%2Fclaude.ai%2Fcb&state=xyz'
  expect(parseAddress('/authorize', query)).toEqual({ screen: 'authorize', query })
  expect(inShell({ screen: 'authorize', query })).toBe(false)
})
