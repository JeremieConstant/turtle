import type { Extensions } from '@tiptap/core'
import { TaskItem, TaskList } from '@tiptap/extension-list'
import StarterKit from '@tiptap/starter-kit'
import { Callout } from './callout'
import { Image, type ImageOptions } from './image'
import { PageLink } from './pageLink'

/**
 * What a Page's content can hold in the editor: only what Markdown can
 * express (ADR-0006). The server reads and writes the same element and
 * attribute names in `server/src/ydoc.rs`;
 * `web/scripts/editor-contract.test.ts` holds the two to each other.
 */
export function contentExtensions(image: Partial<ImageOptions> = {}): Extensions {
  return [
    // Underline is off because Markdown cannot express it (ADR-0006).
    // The trailing node is off because every open editor would append its
    // own empty paragraph at the same moment, and they would pile up.
    StarterKit.configure({ undoRedo: false, underline: false, trailingNode: false }),
    PageLink,
    TaskList,
    // A task may hold more than a line, as it can in Markdown.
    TaskItem.configure({ nested: true }),
    Callout,
    Image.configure(image),
  ]
}
