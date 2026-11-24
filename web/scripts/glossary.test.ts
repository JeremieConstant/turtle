// GLOSSARY.md defines each term once and lists words to avoid beside it. A
// word cannot be both: these tests catch a term that another entry says to
// avoid, and a word to avoid in a name the code declares.

import { readdirSync, readFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'

const GLOSSARY = fileURLToPath(new URL('../../GLOSSARY.md', import.meta.url))
const ROOT = fileURLToPath(new URL('../..', import.meta.url))

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

/**
 * Words the glossary avoids that also have a sense in programming, such as a
 * block of code or a DOM node, so code may use them in that sense. Review
 * still judges whether a name means the glossary's sense.
 */
const PROGRAMMING_WORDS = new Set([
  'account', 'archive', 'backup', 'block', 'chapter', 'directory', 'document', 'download',
  'draft', 'dump', 'element', 'extension', 'home', 'item', 'label', 'layout', 'node', 'note',
  'owner', 'panel', 'report', 'root', 'secret', 'share', 'site', 'space', 'summary', 'upload',
  'user',
])

/** The one-word terms the glossary says to avoid. */
function avoidedWords(glossary: string): Set<string> {
  return new Set(
    [...glossary.matchAll(/^_Avoid_: (.+)$/gm)]
      .flatMap(match => match[1].split(',').map(word => word.trim().toLowerCase()))
      .filter(word => /^[a-z]+$/.test(word)),
  )
}

const DECLARATION =
  /\b(?:fn|struct|enum|type|const|static|mod|trait|function|let|interface|class)\s+([A-Za-z_]\w*)|^\s*\.([a-z][a-z0-9-]*)[\s,{:]/gm

/** The names a source file declares: its own name, functions, types, variables and CSS classes. */
function declaredNames(file: string, source: string): string[] {
  return [basename(file).replace(/\..*$/, ''), ...[...source.matchAll(DECLARATION)].map(match => match[1] ?? match[2])]
}

/** Each avoided word in a declared name, as "word in name (file)". */
function avoidedInNames(avoided: Set<string>, file: string, source: string): string[] {
  return declaredNames(file, source).flatMap(name =>
    name
      .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
      .split(/[\s_-]+/)
      .map(word => word.toLowerCase())
      .map(word => (avoided.has(word) ? word : word.endsWith('s') && avoided.has(word.slice(0, -1)) ? word.slice(0, -1) : null))
      .filter((word): word is string => word !== null && !PROGRAMMING_WORDS.has(word))
      .map(word => `${word} in ${name} (${file})`),
  )
}

function sourceFiles(dir: string): string[] {
  return readdirSync(join(ROOT, dir), { withFileTypes: true, recursive: true })
    .filter(entry => entry.isFile() && /\.(rs|ts|svelte|css)$/.test(entry.name) && !entry.name.endsWith('.gen.ts'))
    .map(entry => join(entry.parentPath, entry.name).slice(ROOT.length))
}

test('an avoided word is found in declared names, however they are cased, but not in text', () => {
  const avoided = avoidedWords('**Status**:\nA marker.\n_Avoid_: badge, lozenge, label\n')
  const source = `export type Badge = { label: string }
function badgeOf(x) { return 'a badge' }
.lozenges { color: red }
const statusLabel = 1
`
  expect(avoidedInNames(avoided, 'web/src/lib/Chip.svelte', source)).toEqual([
    'badge in Badge (web/src/lib/Chip.svelte)',
    'badge in badgeOf (web/src/lib/Chip.svelte)',
    'lozenge in lozenges (web/src/lib/Chip.svelte)',
  ])
})

test('no name the code declares uses a word the glossary says to avoid', () => {
  const avoided = avoidedWords(readFileSync(GLOSSARY, 'utf8'))
  const found = ['server/src', 'web/src'].flatMap(sourceFiles).flatMap(file =>
    avoidedInNames(avoided, file, readFileSync(join(ROOT, file), 'utf8')),
  )
  expect(found).toEqual([])
})
