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
 * The Quick Note a Member has written but not saved yet, kept in this browser
 * so it survives reloading the tab. Each Member who signs in here has their
 * own. Storage can be refused, as in a private window; the text is then lost
 * on reload, and nothing else goes wrong.
 */
export function unsavedQuickNote(memberId: string, storage: UnsavedStorage | null = browserStorage()) {
  const key = `turtle.unsaved-quick-note.${memberId}`
  return {
    read(): string {
      try {
        return storage?.getItem(key) ?? ''
      } catch {
        return ''
      }
    },
    keep(text: string) {
      try {
        if (text.trim()) storage?.setItem(key, text)
        else storage?.removeItem(key)
      } catch {
        // Losing unsaved text on reload is better than refusing to type.
      }
    },
  }
}
