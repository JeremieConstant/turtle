import * as Y from 'yjs'

/** The part of the browser's storage unsaved text needs. */
export type UnsavedStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>

/** The browser's own storage, or none where reaching it already throws. */
function browserStorage(): UnsavedStorage | null {
  try {
    return window.localStorage
  } catch {
    return null
  }
}

/**
 * What a Quick Note's body holds: the document its editor writes into, as the
 * server reads it. Its state, in base64, is what the server is sent.
 */
export function stateOf(doc: Y.Doc): string {
  let binary = ''
  for (const byte of Y.encodeStateAsUpdate(doc)) binary += String.fromCharCode(byte)
  return btoa(binary)
}

/** A body document holding what `stateOf` once gave, or an empty one for anything unreadable. */
export function docFrom(state: string): Y.Doc {
  const doc = new Y.Doc()
  if (!state) return doc
  try {
    Y.applyUpdate(doc, Uint8Array.from(atob(state), char => char.charCodeAt(0)))
  } catch {
    return new Y.Doc()
  }
  return doc
}

/** A Quick Note not saved yet: its title, and its body as `stateOf` gives it, empty when nothing was written. */
export type Draft = { title: string; body: string }

const NOTHING: Draft = { title: '', body: '' }

/**
 * The Quick Note a Member has written but not saved yet, kept in this browser
 * so it survives reloading the tab. Each Member who signs in here has their
 * own. Storage can be refused, as in a private window; the draft is then lost
 * on reload, and nothing else goes wrong.
 */
export function unsavedQuickNote(memberId: string, storage: UnsavedStorage | null = browserStorage()) {
  const key = `turtle.unsaved-quick-note.${memberId}`
  return {
    read(): Draft {
      try {
        const kept = storage?.getItem(key)
        if (!kept) return NOTHING
        const draft = JSON.parse(kept)
        // A draft kept before Quick Notes had a title of their own was plain text.
        if (typeof draft?.title !== 'string') return NOTHING
        return { title: draft.title, body: typeof draft.body === 'string' ? draft.body : '' }
      } catch {
        return NOTHING
      }
    },
    keep(draft: Draft) {
      try {
        if (draft.title.trim() || draft.body) storage?.setItem(key, JSON.stringify(draft))
        else storage?.removeItem(key)
      } catch {
        // Losing an unsaved draft on reload is better than refusing to type.
      }
    },
  }
}
