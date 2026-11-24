import { Node, type Extensions } from '@tiptap/core'
import { TaskItem, TaskList } from '@tiptap/extension-list'
import StarterKit from '@tiptap/starter-kit'
import { Callout } from './callout'
import { TOP_BLOCK, UnknownDirective, UnknownDirectiveBlock } from './directive'
import { Image, type ImageOptions } from './image'
import { Mention } from './mention'
import { PageLink } from './pageLink'
import { Status } from './status'
import { TABLE_EXTENSIONS } from './table'

/** A Page's content: blocks, and those that stand only at its top. */
const Document = Node.create({
  name: 'doc',
  topNode: true,
  content: `(block | ${TOP_BLOCK})+`,
})

/**
 * What a Page's content can hold in the editor: only what Markdown can
 * express (ADR-0006), with directives (ADR-0013). The server reads and
 * writes the same element and attribute names in `server/src/ydoc.rs`;
 * `web/scripts/editor-contract.test.ts` holds the two to each other.
 */
export function contentExtensions(image: Partial<ImageOptions> = {}, pageId: string | null = null): Extensions {
  return [
    // Underline is off because Markdown cannot express it (ADR-0006).
    // The trailing node is off because every open editor would append its
    // own empty paragraph at the same moment, and they would pile up.
    StarterKit.configure({ document: false, undoRedo: false, underline: false, trailingNode: false }),
    Document,
    PageLink,
    Mention.configure({ pageId }),
    TaskList,
    // A task may hold more than a line, as it can in Markdown.
    TaskItem.configure({ nested: true }),
    Callout,
    ...TABLE_EXTENSIONS,
    Image.configure(image),
    Status,
    UnknownDirective,
    UnknownDirectiveBlock,
  ]
}
