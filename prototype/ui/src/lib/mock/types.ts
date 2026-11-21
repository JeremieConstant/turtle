/*
 * The shapes the mock operations return. They follow the names of the real
 * operation catalogue (server, ADR-0005) where one exists, so the screens can
 * move to the generated types in web/src/lib/operations.gen.ts later.
 */

export type Role = 'read' | 'write' | 'admin'
export type Kind = 'page' | 'folder'

export type Author =
  | { kind: 'member'; id: string; name: string }
  | { kind: 'agent'; id: string; name: string }

export type Area = {
  id: string
  name: string
  /** The Member's own Area, where Quick Notes land. */
  personal: boolean
  /** The Role the signed-in Member holds on this Area. */
  role: Role
}

export type Entry = {
  id: string
  area_id: string
  parent_id: string | null
  kind: Kind
  title: string
  /** Order among siblings. */
  position: number
}

export type TreeEntry = Entry & { children: TreeEntry[] }

export type Page = {
  id: string
  title: string
  area: Area
  /** Folders and Pages above this Page, outermost first. */
  path: { id: string; kind: Kind; title: string }[]
  /** TipTap HTML. The real server keeps a CRDT and hands out Markdown (ADR-0001). */
  html: string
  edited_at: number
  edited_by: Author
  /** The Member's Role on this Page, inherited from its Area. */
  role: Role
}

export type QuickNote = {
  id: string
  title: string
  text: string
  created_at: number
  /** The Area the note sits in now; the Personal Area until it is moved. */
  area_id: string
}

export type Fragment = { text: string; match: boolean }

export type SearchHit = {
  id: string
  kind: Kind
  title: string
  area: { id: string; name: string }
  path: string[]
  snippet: Fragment[]
}
