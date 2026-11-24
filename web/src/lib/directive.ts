import { Node } from '@tiptap/core'

/**
 * The name of the directive a source starts with, after its colons, as the
 * server reads it (`server/src/directive.rs`): words joined by dots, such as
 * `acme.board` (ADR-0013).
 */
export function directiveName(source: string): string {
  return /^:+([A-Za-z][\w-]*(?:\.[A-Za-z][\w-]*)*)/.exec(source)?.[1] ?? ''
}

/** What the placeholder for a directive Turtle does not know says. */
export function placeholderText(source: string): string {
  return `Unknown element: ${directiveName(source)}`
}

/** The group of blocks that stand only at the top of a Page, where Markdown can hold them. */
export const TOP_BLOCK = 'topBlock'

const source = {
  source: {
    default: '',
    parseHTML: (element: HTMLElement) => element.getAttribute('data-source') ?? '',
    renderHTML: (attributes: Record<string, unknown>) => ({ 'data-source': attributes.source }),
  },
}

/**
 * A directive in running text that Turtle does not know, such as one from a
 * plugin this installation lacks. It keeps its source exactly as written and
 * shows as a locked placeholder naming it: it can be selected, moved and
 * deleted, never edited.
 */
export const UnknownDirective = Node.create({
  name: 'unknownDirective',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  draggable: true,
  marks: '',

  addAttributes() {
    return source
  },

  parseHTML() {
    return [{ tag: 'span[data-unknown-directive]' }]
  },

  renderHTML({ node, HTMLAttributes }) {
    const text = placeholderText(String(node.attrs.source))
    return ['span', { ...HTMLAttributes, 'data-unknown-directive': '', class: 'unknown-directive', title: text }, text]
  },
})

/** A block directive Turtle does not know, kept and shown as `UnknownDirective` is. */
export const UnknownDirectiveBlock = Node.create({
  name: 'unknownDirectiveBlock',
  group: TOP_BLOCK,
  atom: true,
  selectable: true,
  draggable: true,

  addAttributes() {
    return source
  },

  parseHTML() {
    return [{ tag: 'div[data-unknown-directive]' }]
  },

  renderHTML({ node, HTMLAttributes }) {
    const text = placeholderText(String(node.attrs.source))
    return ['div', { ...HTMLAttributes, 'data-unknown-directive': '', class: 'unknown-directive' }, text]
  },
})
