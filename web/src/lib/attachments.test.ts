import { expect, test } from 'vitest'
import { base64Of, fileSize } from './attachments'

test('a size reads in bytes, KB or MB, as people read file sizes', () => {
  expect(fileSize(0)).toBe('0 bytes')
  expect(fileSize(1)).toBe('1 byte')
  expect(fileSize(1023)).toBe('1023 bytes')
  expect(fileSize(1536)).toBe('1.5 KB')
  expect(fileSize(25 * 1024 * 1024)).toBe('25 MB')
  expect(fileSize(10 * 1024 * 1024 + 300 * 1024)).toBe('10.3 MB')
})

test('content is sent in base64', () => {
  expect(base64Of(new TextEncoder().encode('hello'))).toBe('aGVsbG8=')
  expect(base64Of(new Uint8Array())).toBe('')
})

test('large content is encoded whole, beyond what one call of the browser can take', () => {
  const large = new Uint8Array(300_000).fill(97)
  expect(base64Of(large)).toBe('YWFh'.repeat(100_000))
})
