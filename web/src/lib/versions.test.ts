import { expect, test } from 'vitest'
import { authorNames, previousNumber, versionTitle } from './versions'

const person = (name: string) => ({ id: name.toLowerCase(), name })

test('authors are named the way a sentence would list them', () => {
  expect(authorNames([person('Ada')])).toBe('Ada')
  expect(authorNames([person('Ada'), person('Grace')])).toBe('Ada and Grace')
  expect(authorNames([person('Ada'), person('Grace'), person('Linus')])).toBe('Ada, Grace and Linus')
  // An author whose account is gone is no longer listed.
  expect(authorNames([])).toBe('Unknown')
})

test('a restored Version says which one it brought back', () => {
  expect(versionTitle({ number: 3, restored_from: null })).toBe('Version 3')
  expect(versionTitle({ number: 4, restored_from: 1 })).toBe('Version 4, restored from Version 1')
})

test('a Version is compared with the one recorded before it, if any', () => {
  const listed = [{ number: 5 }, { number: 4 }, { number: 2 }]
  expect(previousNumber(listed, 5)).toBe(4)
  expect(previousNumber(listed, 4)).toBe(2)
  expect(previousNumber(listed, 2)).toBeNull()
  expect(previousNumber(listed, 3)).toBeNull()
})
