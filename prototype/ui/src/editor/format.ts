/*
 * Everything the editor can format, in one list, so the toolbar and the "/"
 * menu always offer the same things with the same names and icons. Only
 * what Markdown can express (ADR-0006).
 */
import type { Editor } from '@tiptap/core'
import type { IconName } from '../components/Icon.svelte'

export type Pick = 'page' | 'image'

export type Format = {
  id: string
  label: string
  icon: IconName
  /** Extra words the "/" menu matches on. */
  keys?: string
  /** Shown in the toolbar's tooltip. */
  shortcut?: string
  active?: (e: Editor) => boolean
  can?: (e: Editor) => boolean
  run: (e: Editor, pick: (what: Pick) => void) => void
}

const mac = typeof navigator !== 'undefined' && /Mac|iP(hone|ad)/.test(navigator.platform)
const key = (combo: string) => (mac ? combo.replace('Mod', '⌘').replace('Alt', '⌥').replace('Shift', '⇧').replaceAll('+', '') : combo.replace('Mod', 'Ctrl'))

/** The kind of paragraph the cursor is in. */
export const STYLES: Format[] = [
  {
    id: 'text',
    label: 'Text',
    icon: 'text',
    shortcut: key('Mod+Alt+0'),
    active: (e) => e.isActive('paragraph'),
    run: (e) => e.chain().focus().setParagraph().run(),
  },
  {
    id: 'h2',
    label: 'Heading',
    icon: 'h2',
    keys: 'h2 heading title',
    shortcut: key('Mod+Alt+2'),
    active: (e) => e.isActive('heading', { level: 2 }),
    run: (e) => e.chain().focus().setHeading({ level: 2 }).run(),
  },
  {
    id: 'h3',
    label: 'Subheading',
    icon: 'h3',
    keys: 'h3 subheading',
    shortcut: key('Mod+Alt+3'),
    active: (e) => e.isActive('heading', { level: 3 }),
    run: (e) => e.chain().focus().setHeading({ level: 3 }).run(),
  },
]

/** Marks on the selected text. */
export const MARKS: Format[] = [
  {
    id: 'bold',
    label: 'Bold',
    icon: 'bold',
    shortcut: key('Mod+B'),
    active: (e) => e.isActive('bold'),
    can: (e) => e.can().toggleBold(),
    run: (e) => e.chain().focus().toggleBold().run(),
  },
  {
    id: 'italic',
    label: 'Italic',
    icon: 'italic',
    shortcut: key('Mod+I'),
    active: (e) => e.isActive('italic'),
    can: (e) => e.can().toggleItalic(),
    run: (e) => e.chain().focus().toggleItalic().run(),
  },
  {
    id: 'strike',
    label: 'Strikethrough',
    icon: 'strike',
    shortcut: key('Mod+Shift+S'),
    active: (e) => e.isActive('strike'),
    can: (e) => e.can().toggleStrike(),
    run: (e) => e.chain().focus().toggleStrike().run(),
  },
  {
    id: 'code',
    label: 'Code',
    icon: 'code',
    shortcut: key('Mod+E'),
    active: (e) => e.isActive('code'),
    can: (e) => e.can().toggleCode(),
    run: (e) => e.chain().focus().toggleCode().run(),
  },
  {
    id: 'link',
    label: 'Link',
    icon: 'link',
    active: (e) => e.isActive('link'),
    // Inside a link the same button takes it off again.
    run: (e, pick) => (e.isActive('link') ? e.chain().focus().extendMarkRange('link').unsetLink().run() : pick('page')),
  },
]

export const LISTS: Format[] = [
  {
    id: 'bullets',
    label: 'Bulleted list',
    icon: 'bullets',
    keys: 'bullet list ul',
    shortcut: key('Mod+Shift+8'),
    active: (e) => e.isActive('bulletList'),
    run: (e) => e.chain().focus().toggleBulletList().run(),
  },
  {
    id: 'numbers',
    label: 'Numbered list',
    icon: 'numbers',
    keys: 'numbered ordered list ol',
    shortcut: key('Mod+Shift+7'),
    active: (e) => e.isActive('orderedList'),
    run: (e) => e.chain().focus().toggleOrderedList().run(),
  },
  {
    id: 'tasks',
    label: 'Task list',
    icon: 'tasks',
    keys: 'task todo checkbox',
    shortcut: key('Mod+Shift+9'),
    active: (e) => e.isActive('taskList'),
    run: (e) => e.chain().focus().toggleTaskList().run(),
  },
]

export const BLOCKS: Format[] = [
  {
    id: 'quote',
    label: 'Quote',
    icon: 'quote',
    keys: 'quote blockquote',
    active: (e) => e.isActive('blockquote'),
    run: (e) => e.chain().focus().toggleBlockquote().run(),
  },
  {
    id: 'codeblock',
    label: 'Code block',
    icon: 'codeblock',
    keys: 'code pre snippet',
    active: (e) => e.isActive('codeBlock'),
    run: (e) => e.chain().focus().toggleCodeBlock().run(),
  },
  {
    id: 'callout',
    label: 'Callout',
    icon: 'callout',
    keys: 'callout note info',
    active: (e) => e.isActive('callout'),
    run: (e) => (e.isActive('callout') ? e.chain().focus().lift('callout').run() : e.chain().focus().wrapIn('callout').run()),
  },
]

export const INSERTS: Format[] = [
  {
    id: 'table',
    label: 'Table',
    icon: 'table',
    keys: 'table grid',
    can: (e) => e.can().insertTable({ rows: 3, cols: 3, withHeaderRow: true }),
    run: (e) => e.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(),
  },
  { id: 'image', label: 'Image', icon: 'image', keys: 'image picture', run: (_e, pick) => pick('image') },
  { id: 'page', label: 'Link to a Page', icon: 'page', keys: 'link page', run: (_e, pick) => pick('page') },
]

/** What "/" offers: every kind of block, nothing that formats selected text. */
export const SLASH: Format[] = [...STYLES.slice(1), ...LISTS, ...INSERTS.slice(0, 1), ...BLOCKS, ...INSERTS.slice(2), INSERTS[1]]
