/*
 * Everything the editor can format, in one list, with the names and icons
 * the toolbar shows. Only what Markdown can express (ADR-0006), and only
 * what the editor's extensions support today.
 */
import type { Editor } from '@tiptap/core'
import type { IconName } from '../components/Icon.svelte'
import { CALLOUT_KINDS, CALLOUT_LABELS } from './callout'

export type Format = {
  id: string
  label: string
  icon: IconName
  /** Shown with the label in the tooltip. */
  shortcut?: string
  /** Whether it applies where the cursor is. */
  active?: (editor: Editor) => boolean
  run: (editor: Editor) => void
}

const mac = typeof navigator !== 'undefined' && /Mac|iP(hone|ad)/.test(navigator.platform)

/** A shortcut as this computer writes it: "⌘⌥2" on a Mac, "Ctrl+Alt+2" elsewhere. */
export function shortcut(combo: string): string {
  if (!mac) return combo.replace('Mod', 'Ctrl')
  return combo.replace('Mod', '⌘').replace('Alt', '⌥').replace('Shift', '⇧').replaceAll('+', '')
}

/** The kind of paragraph the cursor is in; exactly one applies at a time. */
export const STYLES: Format[] = [
  {
    id: 'text',
    label: 'Text',
    icon: 'text',
    shortcut: shortcut('Mod+Alt+0'),
    active: e => e.isActive('paragraph'),
    run: e => e.chain().focus().setParagraph().run(),
  },
  ...([1, 2, 3] as const).map(level => ({
    id: `h${level}`,
    label: `Heading ${level}`,
    icon: `h${level}` as IconName,
    shortcut: shortcut(`Mod+Alt+${level}`),
    active: (e: Editor) => e.isActive('heading', { level }),
    run: (e: Editor) => e.chain().focus().setHeading({ level }).run(),
  })),
]

/** Marks on the selected text. Link is not here: it needs an address first. */
export const MARKS: Format[] = [
  {
    id: 'bold',
    label: 'Bold',
    icon: 'bold',
    shortcut: shortcut('Mod+B'),
    active: e => e.isActive('bold'),
    run: e => e.chain().focus().toggleBold().run(),
  },
  {
    id: 'italic',
    label: 'Italic',
    icon: 'italic',
    shortcut: shortcut('Mod+I'),
    active: e => e.isActive('italic'),
    run: e => e.chain().focus().toggleItalic().run(),
  },
  {
    id: 'strike',
    label: 'Strikethrough',
    icon: 'strike',
    shortcut: shortcut('Mod+Shift+S'),
    active: e => e.isActive('strike'),
    run: e => e.chain().focus().toggleStrike().run(),
  },
  {
    id: 'code',
    label: 'Code',
    icon: 'code',
    shortcut: shortcut('Mod+E'),
    active: e => e.isActive('code'),
    run: e => e.chain().focus().toggleCode().run(),
  },
]

export const LISTS: Format[] = [
  {
    id: 'bullets',
    label: 'Bulleted list',
    icon: 'bullets',
    shortcut: shortcut('Mod+Shift+8'),
    active: e => e.isActive('bulletList'),
    run: e => e.chain().focus().toggleBulletList().run(),
  },
  {
    id: 'numbers',
    label: 'Numbered list',
    icon: 'numbers',
    shortcut: shortcut('Mod+Shift+7'),
    active: e => e.isActive('orderedList'),
    run: e => e.chain().focus().toggleOrderedList().run(),
  },
  {
    id: 'tasks',
    label: 'Task list',
    icon: 'tasks',
    shortcut: shortcut('Mod+Shift+9'),
    active: e => e.isActive('taskList'),
    run: e => e.chain().focus().toggleTaskList().run(),
  },
]

/** The kinds of callout; the one the cursor is in applies, and choosing it again takes the callout away. */
export const CALLOUTS: Format[] = CALLOUT_KINDS.map(kind => ({
  id: `callout-${kind}`,
  label: CALLOUT_LABELS[kind],
  icon: 'callout',
  active: (e: Editor) => e.isActive('callout', { kind }),
  run: (e: Editor) => e.chain().focus().toggleCallout(kind).run(),
}))

export const BLOCKS: Format[] = [
  {
    id: 'quote',
    label: 'Quote',
    icon: 'quote',
    shortcut: shortcut('Mod+Shift+B'),
    active: e => e.isActive('blockquote'),
    run: e => e.chain().focus().toggleBlockquote().run(),
  },
  {
    id: 'codeblock',
    label: 'Code block',
    icon: 'codeblock',
    shortcut: shortcut('Mod+Alt+C'),
    active: e => e.isActive('codeBlock'),
    run: e => e.chain().focus().toggleCodeBlock().run(),
  },
  {
    id: 'rule',
    label: 'Divider',
    icon: 'rule',
    run: e => e.chain().focus().setHorizontalRule().run(),
  },
]
