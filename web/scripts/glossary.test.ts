// GLOSSARY.md defines each term once and lists words to avoid beside it. A
// word cannot be both: these tests catch a term that another entry says to
// avoid.

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'

const GLOSSARY = fileURLToPath(new URL('../../GLOSSARY.md', import.meta.url))

/** Each term the glossary avoids that it also defines, as "Term avoided by Entry". */
function contradictions(glossary: string): string[] {
  const entries = [...glossary.matchAll(/^\*\*(.+?)\*\*:\n[\s\S]*?^_Avoid_: (.+)$/gm)].map(match => ({
    term: match[1],
    avoided: match[2].split(',').map(word => word.trim().toLowerCase()),
  }))
  const terms = new Map(entries.map(({ term }) => [term.toLowerCase(), term]))
  return entries.flatMap(({ term, avoided }) =>
    avoided.flatMap(word => (terms.has(word) ? [`${terms.get(word)} avoided by ${term}`] : [])),
  )
}

test('a term another entry says to avoid is found, whatever its case', () => {
  const glossary = `**Folder**:
A container.
_Avoid_: directory, section

**Section**:
A heading and what follows it.
_Avoid_: chapter
`
  expect(contradictions(glossary)).toEqual(['Section avoided by Folder'])
})

test('no term of the glossary is on the list of words to avoid', () => {
  expect(contradictions(readFileSync(GLOSSARY, 'utf8'))).toEqual([])
})
