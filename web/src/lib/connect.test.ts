import { expect, test } from 'vitest'
import { connectionRequest } from './connect'

test('the consent page reads the request the app sent', () => {
  const query =
    '?client_id=c1&redirect_uri=https%3A%2F%2Fclaude.ai%2Fapi%2Fmcp%2Fauth_callback&code_challenge=abc&code_challenge_method=S256&state=a%20b&scope=turtle%3Aread%20turtle%3Awrite'
  expect(connectionRequest(query)).toEqual({
    client_id: 'c1',
    redirect_uri: 'https://claude.ai/api/mcp/auth_callback',
    code_challenge: 'abc',
    code_challenge_method: 'S256',
    state: 'a b',
    scope: 'turtle:read turtle:write',
  })
})

test('what the app left out stays out', () => {
  expect(connectionRequest('?client_id=c1')).toEqual({
    client_id: 'c1',
    redirect_uri: null,
    code_challenge: null,
    code_challenge_method: null,
    state: null,
    scope: null,
  })
})

test('without a client ID there is no request', () => {
  expect(connectionRequest('')).toBeNull()
  expect(connectionRequest('?state=x')).toBeNull()
})
