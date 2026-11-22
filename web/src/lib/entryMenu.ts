/*
 * What the Entry menu offers, in the sidebar and on the Area screen. Each
 * item is one operation, except copying a link, which needs none.
 */
import { toast } from 'svelte-sonner'
import type { Kind, Role } from './operations.gen'
import { pageAddress } from './routes'

export type EntryMenuItem = 'new-page' | 'new-folder' | 'rename' | 'copy-link'

/**
 * The items of the menu of an Entry, in order, for someone holding `role` on
 * its Area. Only Pages have an address, so only they offer a link; a Folder
 * seen with Read therefore offers nothing.
 */
export function entryMenuItems(kind: Kind, role: Role): EntryMenuItem[] {
  const changing: EntryMenuItem[] = role === 'read' ? [] : ['new-page', 'new-folder', 'rename']
  return kind === 'page' ? [...changing, 'copy-link'] : changing
}

/**
 * Puts the full address of a Page on the clipboard and confirms it in a
 * toast. The clipboard is missing outside secure origins and may be refused,
 * and then the toast shows the address to copy by hand.
 */
export async function copyLink(
  page: { id: string; title: string },
  origin: string = location.origin,
  clipboard: Pick<Clipboard, 'writeText'> | undefined = navigator.clipboard,
) {
  const address = origin + pageAddress(page.id, page.title)
  const failed = () => toast('The link could not be copied. Copy it from here.', { description: address })
  if (!clipboard) return failed()
  try {
    await clipboard.writeText(address)
    toast('Link copied', { description: page.title })
  } catch {
    failed()
  }
}
