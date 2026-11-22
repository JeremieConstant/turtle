import { expect, test } from 'vitest'
import { suggestedTags } from './tags'

const inUse = ['runbook', 'production', 'release-notes', 'onboarding', 'deploy']

test('every Tag in use that the Page lacks is suggested before anything is typed, in the order given', () => {
  expect(suggestedTags(inUse, ['production'], '')).toEqual(['runbook', 'release-notes', 'onboarding', 'deploy'])
})

test('what is typed narrows the suggestions whatever its case, with or without a leading #', () => {
  expect(suggestedTags(inUse, [], 'ON')).toEqual(['production', 'onboarding'])
  expect(suggestedTags(inUse, [], '#Run')).toEqual(['runbook'])
})

test('spaces typed match the hyphens a Tag is stored with', () => {
  expect(suggestedTags(inUse, [], 'release no')).toEqual(['release-notes'])
})

test('at most six Tags are suggested', () => {
  const many = ['a1', 'a2', 'a3', 'a4', 'a5', 'a6', 'a7']
  expect(suggestedTags(many, [], 'a')).toEqual(['a1', 'a2', 'a3', 'a4', 'a5', 'a6'])
})
