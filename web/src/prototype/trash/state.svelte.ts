/*
 * PROTOTYPE, not production (issue #14). Three variants of where the Trash
 * lives and what a deleted Subtree looks like, switchable via ?variant= on
 * /prototype/trash, inside the real shell.
 *
 * Nothing here talks to the server: the deleted Entries are made up, live in
 * memory and come back on reload. They are spread over the real Areas of the
 * person so the names and the density are true.
 */
import { toast } from 'svelte-sonner'
import type { Kind } from '../../lib/operations.gen'
import { navigation } from '../../lib/navigation.svelte'

export const VARIANTS = [
  { key: 'A', name: 'Trash per Area' },
  { key: 'B', name: 'One Trash for the Workspace' },
  { key: 'C', name: 'Deleted stay in the tree' },
] as const
export type VariantKey = (typeof VARIANTS)[number]['key']

export const variantOf = (raw: string | null): VariantKey => VARIANTS.find(v => v.key === raw)?.key ?? 'A'

/** The variant last looked at, so the sidebar can show that variant's way in on every screen. */
export const proto = $state<{ variant: VariantKey }>({ variant: 'A' })

/** Only where someone is trying things out: a dev server or a loopback address. */
export const prototyping = import.meta.env.DEV || ['127.0.0.1', 'localhost', '[::1]'].includes(location.hostname)

export const KEEP_DAYS = 30

export type Node = { kind: Kind; title: string; children: Node[] }
export type Trashed = Node & {
  id: string
  /** Which of the person's Areas it was deleted from, counted through the list. */
  area: number
  /** The Folders and Pages it sat beneath, outermost first. */
  was: string[]
  /** True when what it sat beneath has been deleted for good since. */
  parentGone: boolean
  by: string
  daysAgo: number
}

const page = (title: string, children: Node[] = []): Node => ({ kind: 'page', title, children })
const folder = (title: string, children: Node[] = []): Node => ({ kind: 'folder', title, children })

const SEED: Trashed[] = [
  { id: 't1', area: 0, was: ['Runbooks'], parentGone: false, by: 'Ada Example', daysAgo: 0, ...page('Rollback (old)') },
  {
    id: 't2',
    area: 0,
    was: [],
    parentGone: false,
    by: 'Lena Fischer',
    daysAgo: 2,
    ...folder('Migration 2025', [
      page('Plan', [page('Cut-over checklist'), page('Rollback plan')]),
      page('Retrospective'),
      folder('Vendor notes', [page('Offer A'), page('Offer B'), page('Decision')]),
    ]),
  },
  { id: 't3', area: 0, was: ['Runbooks', 'Deploying to production'], parentGone: false, by: 'Docs Agent', daysAgo: 6, ...page('Draft: canary releases') },
  { id: 't4', area: 1, was: ['Onboarding'], parentGone: true, by: 'Jonas Weber', daysAgo: 11, ...page('First week (2024)', [page('Laptop setup'), page('Accounts')]) },
  { id: 't5', area: 1, was: [], parentGone: false, by: 'Ada Example', daysAgo: 17, ...page('Holiday rules, superseded') },
  { id: 't6', area: 0, was: ['Runbooks'], parentGone: false, by: 'Lena Fischer', daysAgo: 24, ...page('On-call rota Q1') },
  { id: 't7', area: 1, was: ['Policies'], parentGone: false, by: 'Jonas Weber', daysAgo: 27, ...folder('Travel', [page('Booking'), page('Expenses')]) },
  { id: 't8', area: 0, was: [], parentGone: false, by: 'Ada Example', daysAgo: 29, ...page('Scratch: a very long title that somebody typed in a hurry and never came back to shorten') },
  { id: 't9', area: 2, was: [], parentGone: false, by: 'Ada Example', daysAgo: 1, ...page('Meeting notes, Tuesday') },
]

export const trash = $state<{ items: Trashed[] }>({ items: structuredClone(SEED) })

export const daysLeft = (item: Trashed) => KEEP_DAYS - item.daysAgo
export const beneath = (node: Node): number => node.children.reduce((sum, child) => sum + 1 + beneath(child), 0)

export function ago(days: number): string {
  if (days === 0) return 'today'
  if (days === 1) return 'yesterday'
  return `${days} days ago`
}

export function leaves(item: Trashed): string {
  const left = daysLeft(item)
  if (left <= 1) return 'Gone tomorrow'
  return `${left} days left`
}

/** The Area a made-up Entry is counted to: the person's own Areas, in order, round and round. */
export function areaOf(item: Trashed) {
  // Shared Areas first, so most of the made-up Entries land where a team works.
  const all = navigation.areas ?? []
  const areas = [...all.filter(area => !area.personal), ...all.filter(area => area.personal)]
  return areas.length ? areas[item.area % areas.length] : null
}

export const inArea = (areaId: string) => trash.items.filter(item => areaOf(item)?.id === areaId)

/** Stub for `restore_entry`: the Entry leaves the Trash, and the toast can undo it. */
export function restore(ids: string[]) {
  const back = trash.items.filter(item => ids.includes(item.id))
  if (!back.length) return
  trash.items = trash.items.filter(item => !ids.includes(item.id))
  const one = back[0]
  const where = one.parentGone || !one.was.length ? `the top of ${areaOf(one)?.name ?? 'its Area'}` : one.was.join(' / ')
  toast(back.length === 1 ? `Restored to ${where}` : `${back.length} Entries restored`, {
    description: back.length === 1 ? one.title : undefined,
    action: { label: 'Undo', onClick: () => (trash.items = [...trash.items, ...back].sort((a, b) => a.daysAgo - b.daysAgo)) },
  })
}

/** Stub for deleting for good. No undo, which is the point. */
export function purge(ids: string[]) {
  const count = trash.items.filter(item => ids.includes(item.id)).length
  trash.items = trash.items.filter(item => !ids.includes(item.id))
  toast(count === 1 ? 'Deleted for good' : `${count} Entries deleted for good`)
}

export function reset() {
  trash.items = structuredClone(SEED)
}
