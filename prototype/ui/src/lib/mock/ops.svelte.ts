/*
 * The mock operation catalogue. Every action in the interface goes through
 * one of these functions, and each is named after an operation an Agent could
 * call too (ADR-0005). Changes live in memory and are gone on reload.
 *
 * The real operations are asynchronous calls to the server; these are
 * synchronous so the screens can read them inside $derived and update when
 * the in-memory store changes.
 */
import { AREAS, buildRecords, DOCS_AGENT, MEMBERS, nextId, WORKSPACE, type Record_ } from './data'
import type { Area, Author, Entry, Fragment, Page, QuickNote, SearchHit, TreeEntry } from './types'

const db = $state({ records: buildRecords(), areas: AREAS })

/* Entries in the Trash are invisible to every operation but restore_entry. */
const live = () => db.records.filter((r) => !r.trashed_at)
const byId = (id: string) => live().find((r) => r.id === id)
const areaOf = (id: string) => db.areas.find((a) => a.id === id)
const childrenOf = (area_id: string, parent_id: string | null) =>
  live().filter((r) => r.area_id === area_id && r.parent_id === parent_id).sort((a, b) => a.position - b.position)
const subtree = (record: Record_): Record_[] => [record, ...live().filter((c) => c.parent_id === record.id).flatMap(subtree)]
const toEntry = ({ id, area_id, parent_id, kind, title, position }: Record_): Entry => ({
  id,
  area_id,
  parent_id,
  kind,
  title,
  position,
})
const canWrite = (area: Area | undefined) => area?.role === 'write' || area?.role === 'admin'
function renumber(area_id: string, parent_id: string | null) {
  childrenOf(area_id, parent_id).forEach((r, i) => (r.position = i))
}
function ancestors(record: Record_) {
  const path: Record_[] = []
  let parent = record.parent_id ? byId(record.parent_id) : undefined
  while (parent) {
    path.unshift(parent)
    parent = parent.parent_id ? byId(parent.parent_id) : undefined
  }
  return path
}
const plain = (html: string) =>
  html
    .replace(/<[^>]+>/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim()

/* ---- Session ---------------------------------------------------------- */

export function get_session() {
  return { workspace: WORKSPACE, member: MEMBERS.me }
}

/* ---- Areas and the tree ----------------------------------------------- */

/** The Areas the Member can read, the Personal Area first. */
export function list_areas(): Area[] {
  return [...db.areas].sort((a, b) => Number(b.personal) - Number(a.personal))
}

export function get_area({ area_id }: { area_id: string }): Area | undefined {
  return areaOf(area_id)
}

export function get_tree({ area_id }: { area_id: string }): TreeEntry[] {
  const build = (parent: string | null): TreeEntry[] =>
    childrenOf(area_id, parent).map((r) => ({ ...toEntry(r), children: build(r.id) }))
  return build(null)
}

/**
 * Moves an Entry with its Subtree to a new place: under `parent_id` (null for
 * the top of the Area) at `index` among its new siblings.
 */
export function move_entry({
  entry_id,
  area_id,
  parent_id,
  index,
}: {
  entry_id: string
  area_id: string
  parent_id: string | null
  index: number
}) {
  const record = byId(entry_id)
  if (!record) throw new Error('No such Entry')
  if (!canWrite(areaOf(record.area_id)) || !canWrite(areaOf(area_id))) throw new Error('Write Role needed')
  if (parent_id) {
    let p: Record_ | undefined = byId(parent_id)
    while (p) {
      if (p.id === entry_id) throw new Error('An Entry cannot move into its own Subtree')
      p = p.parent_id ? byId(p.parent_id) : undefined
    }
  }
  const from = { area: record.area_id, parent: record.parent_id }
  const siblings = childrenOf(area_id, parent_id).filter((r) => r.id !== entry_id)
  siblings.splice(Math.max(0, Math.min(index, siblings.length)), 0, record)
  record.parent_id = parent_id
  if (record.area_id !== area_id) {
    subtree(record).forEach((r) => (r.area_id = area_id))
  }
  siblings.forEach((r, i) => (r.position = i))
  renumber(from.area, from.parent)
}

/* ---- Pages and Folders ------------------------------------------------ */

export function get_page({ page_id }: { page_id: string }): Page | undefined {
  const r = byId(page_id)
  if (!r || r.kind !== 'page') return undefined
  const area = areaOf(r.area_id)!
  return {
    id: r.id,
    title: r.title,
    area,
    path: ancestors(r).map(({ id, kind, title }) => ({ id, kind, title })),
    html: r.html,
    edited_at: r.edited_at,
    edited_by: r.edited_by,
    tags: [...r.tags],
    role: area.role,
  }
}

function create(kind: 'page' | 'folder', area_id: string, parent_id: string | null, title: string, by: Author) {
  if (!canWrite(areaOf(area_id))) throw new Error('Write Role needed')
  const id = nextId()
  const now = Date.now()
  db.records.push({
    id,
    area_id,
    parent_id,
    kind,
    title,
    position: childrenOf(area_id, parent_id).length,
    html: '',
    edited_at: now,
    edited_by: by,
    created_at: now,
    tags: [],
  })
  return id
}

export function create_page({ area_id, parent_id = null, title }: { area_id: string; parent_id?: string | null; title: string }) {
  return create('page', area_id, parent_id, title, MEMBERS.me)
}

export function create_folder({
  area_id,
  parent_id = null,
  title,
}: {
  area_id: string
  parent_id?: string | null
  title: string
}) {
  return create('folder', area_id, parent_id, title, MEMBERS.me)
}

function rename(id: string, kind: 'page' | 'folder', title: string) {
  const r = byId(id)
  if (!r || r.kind !== kind) throw new Error(`No such ${kind === 'page' ? 'Page' : 'Folder'}`)
  if (!canWrite(areaOf(r.area_id))) throw new Error('Write Role needed')
  r.title = title
  r.edited_at = Date.now()
  r.edited_by = MEMBERS.me
}

export function rename_page({ page_id, title }: { page_id: string; title: string }) {
  rename(page_id, 'page', title)
}

export function rename_folder({ folder_id, title }: { folder_id: string; title: string }) {
  rename(folder_id, 'folder', title)
}

/* ---- Trash ------------------------------------------------------------- */
/* Not in the real catalogue yet. The Trash keeps an Entry with its Subtree,
   so it can come back where it was. */

export function trash_entry({ entry_id }: { entry_id: string }) {
  const r = byId(entry_id)
  if (!r) throw new Error('No such Entry')
  if (!canWrite(areaOf(r.area_id))) throw new Error('Write Role needed')
  const at = Date.now()
  subtree(r).forEach((e) => (e.trashed_at = at))
  renumber(r.area_id, r.parent_id)
}

export function restore_entry({ entry_id }: { entry_id: string }) {
  const r = db.records.find((e) => e.id === entry_id)
  if (!r?.trashed_at) throw new Error('Not in the Trash')
  const at = r.trashed_at
  const siblings = childrenOf(r.area_id, r.parent_id)
  db.records.filter((e) => e.trashed_at === at).forEach((e) => delete e.trashed_at)
  siblings.splice(Math.min(r.position, siblings.length), 0, r)
  siblings.forEach((e, i) => (e.position = i))
}

/* ---- Tags -------------------------------------------------------------- */
/* Not in the real catalogue yet. A Tag exists while one Page carries it. */

const normalTag = (tag: string) => tag.trim().replace(/^#/, '').toLowerCase().replace(/\s+/g, '-')

/** Every Tag in the Workspace, the most used first. */
export function list_tags(): string[] {
  const count = new Map<string, number>()
  live().forEach((r) => r.tags.forEach((t) => count.set(t, (count.get(t) ?? 0) + 1)))
  return [...count].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([t]) => t)
}

export function add_tag({ page_id, tag }: { page_id: string; tag: string }) {
  const r = byId(page_id)
  const name = normalTag(tag)
  if (!r || r.kind !== 'page') throw new Error('No such Page')
  if (!canWrite(areaOf(r.area_id))) throw new Error('Write Role needed')
  if (name && !r.tags.includes(name)) r.tags.push(name)
}

export function remove_tag({ page_id, tag }: { page_id: string; tag: string }) {
  const r = byId(page_id)
  if (!r || !canWrite(areaOf(r.area_id))) throw new Error('Write Role needed')
  r.tags = r.tags.filter((t) => t !== tag)
}

/**
 * Replaces a Page's content. The real operation takes Markdown and applies
 * only the difference to the CRDT, so concurrent edits survive (ADR-0006).
 */
export function replace_page({ page_id, html, by = MEMBERS.me }: { page_id: string; html: string; by?: Author }) {
  const r = byId(page_id)
  if (!r || !canWrite(areaOf(r.area_id))) throw new Error('Write Role needed')
  r.html = html
  r.edited_at = Date.now()
  r.edited_by = by
}

/* ---- Quick Notes ------------------------------------------------------ */

/**
 * Writes a note straight into the Member's Personal Area, as a Page at the top
 * of it. The first line becomes its title.
 */
export function create_quick_note({ text }: { text: string }): string {
  const [first, ...rest] = text.trim().split('\n')
  const id = nextId()
  const now = Date.now()
  childrenOf('personal', null).forEach((r) => r.position++)
  db.records.push({
    id,
    area_id: 'personal',
    parent_id: null,
    kind: 'page',
    title: first.trim(),
    position: 0,
    html: rest
      .filter((l) => l.trim())
      .map((l) => `<p>${l.replace(/</g, '&lt;')}</p>`)
      .join(''),
    edited_at: now,
    edited_by: MEMBERS.me,
    created_at: now,
    tags: [],
    quick_note: true,
  })
  return id
}

/** The Member's most recent Quick Notes, wherever they sit now. */
export function list_quick_notes({ limit = 5 }: { limit?: number } = {}): QuickNote[] {
  return live()
    .filter((r) => r.quick_note)
    .sort((a, b) => b.created_at - a.created_at)
    .slice(0, limit)
    .map((r) => ({ id: r.id, title: r.title, text: plain(r.html), created_at: r.created_at, area_id: r.area_id }))
}

/* ---- Search ----------------------------------------------------------- */

/** Finds Pages and Folders by their words; a word written as #tag finds the Pages with that Tag. */
export function search({ query, limit = 20 }: { query: string; limit?: number }): SearchHit[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean)
  const tags = terms.filter((t) => t.startsWith('#') && t.length > 1).map((t) => t.slice(1))
  const words = terms.filter((t) => !t.startsWith('#'))
  if (!words.length && !tags.length) return []
  const scored: { r: Record_; score: number; text: string }[] = []
  for (const r of live()) {
    if (!tags.every((t) => r.tags.includes(t))) continue
    const title = r.title.toLowerCase()
    const text = plain(r.html)
    const lower = text.toLowerCase()
    if (!words.every((w) => title.includes(w) || lower.includes(w))) continue
    const score = tags.length + words.reduce((s, w) => s + (title.includes(w) ? 10 : 0) + (title.startsWith(w) ? 5 : 0) + (lower.includes(w) ? 1 : 0), 0)
    scored.push({ r, score, text })
  }
  return scored
    .sort((a, b) => b.score - a.score || a.r.title.localeCompare(b.r.title))
    .slice(0, limit)
    .map(({ r, text }) => ({
      id: r.id,
      kind: r.kind,
      title: r.title,
      area: { id: r.area_id, name: areaOf(r.area_id)!.name },
      path: ancestors(r).map((a) => a.title),
      snippet: snippet(text, words),
    }))
}

function snippet(text: string, words: string[]): Fragment[] {
  if (!text) return []
  const lower = text.toLowerCase()
  const first = Math.min(...words.map((w) => lower.indexOf(w)).filter((i) => i >= 0), Infinity)
  const start = first === Infinity ? 0 : Math.max(0, text.lastIndexOf(' ', Math.max(0, first - 60)) + 1)
  const slice = (start > 0 ? '… ' : '') + text.slice(start, start + 180) + (start + 180 < text.length ? ' …' : '')
  if (!words.length) return [{ text: slice, match: false }]
  const pattern = new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'gi')
  return slice
    .split(pattern)
    .filter(Boolean)
    .map((part) => ({ text: part, match: words.includes(part.toLowerCase()) }))
}

/* ---- People ------------------------------------------------------------ */

export const people = { ...MEMBERS, docsAgent: DOCS_AGENT }
