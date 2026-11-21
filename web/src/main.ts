import { Editor } from '@tiptap/core'
import Collaboration from '@tiptap/extension-collaboration'
import CollaborationCaret from '@tiptap/extension-collaboration-caret'
import StarterKit from '@tiptap/starter-kit'
import * as Y from 'yjs'
import { Connection, type Status } from './connection'
import './style.css'

const pageId = decodeURIComponent(location.pathname.match(/^\/p\/([^/]+)/)?.[1] ?? 'demo')

const COLORS = ['#c2410c', '#0f766e', '#7c3aed', '#be185d', '#1d4ed8', '#4d7c0f', '#b45309']

function currentUser(): { name: string; color: string } {
  try {
    const stored = sessionStorage.getItem('turtle-user')
    if (stored) return JSON.parse(stored)
  } catch {
    // Storage can be unavailable; a fresh identity is fine then.
  }
  const user = {
    name: `Visitor ${Math.floor(Math.random() * 900) + 100}`,
    color: COLORS[Math.floor(Math.random() * COLORS.length)],
  }
  try {
    sessionStorage.setItem('turtle-user', JSON.stringify(user))
  } catch {
    // See above.
  }
  return user
}

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T

const user = currentUser()
const doc = new Y.Doc()
el('page-name').textContent = pageId
document.title = `${pageId} · Turtle`

const STATUS_TEXT: Record<Status, string> = {
  connecting: 'Connecting…',
  connected: 'Connected',
  disconnected: 'Offline, retrying…',
}

const connection = new Connection(
  `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws/${encodeURIComponent(pageId)}`,
  doc,
  status => {
    el('status').textContent = STATUS_TEXT[status]
    el('status').dataset.state = status
  },
  startEditor,
)

function renderPeople() {
  const people = el('people')
  people.replaceChildren()
  for (const state of connection.awareness.getStates().values()) {
    if (!state.user) continue
    const chip = document.createElement('span')
    chip.className = 'person'
    chip.textContent = state.user.name
    chip.style.setProperty('--person', state.user.color)
    people.append(chip)
  }
}
connection.awareness.on('change', renderPeople)

/** The editor starts only once the stored content is here, so it never writes into an empty page that is about to be filled. */
function startEditor() {
  const editor = new Editor({
    element: el('editor'),
    extensions: [
      // Underline is off because Markdown cannot express it (ADR-0006).
      // The trailing node is off because every open editor would append its
      // own empty paragraph at the same moment, and they would pile up.
      StarterKit.configure({ undoRedo: false, underline: false, trailingNode: false }),
      Collaboration.configure({ document: doc }),
      CollaborationCaret.configure({ provider: connection, user }),
    ],
    autofocus: true,
  })

  const buttons: [label: string, title: string, run: () => void, active?: () => boolean][] = [
    ['B', 'Bold', () => editor.chain().focus().toggleBold().run(), () => editor.isActive('bold')],
    ['I', 'Italic', () => editor.chain().focus().toggleItalic().run(), () => editor.isActive('italic')],
    ['S', 'Strikethrough', () => editor.chain().focus().toggleStrike().run(), () => editor.isActive('strike')],
    ['<>', 'Inline code', () => editor.chain().focus().toggleCode().run(), () => editor.isActive('code')],
    ['H1', 'Heading 1', () => editor.chain().focus().toggleHeading({ level: 1 }).run(), () => editor.isActive('heading', { level: 1 })],
    ['H2', 'Heading 2', () => editor.chain().focus().toggleHeading({ level: 2 }).run(), () => editor.isActive('heading', { level: 2 })],
    ['H3', 'Heading 3', () => editor.chain().focus().toggleHeading({ level: 3 }).run(), () => editor.isActive('heading', { level: 3 })],
    ['• List', 'Bullet list', () => editor.chain().focus().toggleBulletList().run(), () => editor.isActive('bulletList')],
    ['1. List', 'Numbered list', () => editor.chain().focus().toggleOrderedList().run(), () => editor.isActive('orderedList')],
    ['Quote', 'Quote', () => editor.chain().focus().toggleBlockquote().run(), () => editor.isActive('blockquote')],
    ['Code', 'Code block', () => editor.chain().focus().toggleCodeBlock().run(), () => editor.isActive('codeBlock')],
    ['Rule', 'Horizontal rule', () => editor.chain().focus().setHorizontalRule().run()],
    ['Link', 'Link', () => {
      const href = prompt('Link address', editor.getAttributes('link').href ?? 'https://')
      if (href === null) return
      if (href === '') editor.chain().focus().unsetLink().run()
      else editor.chain().focus().extendMarkRange('link').setLink({ href }).run()
    }, () => editor.isActive('link')],
  ]

  const toolbar = el('toolbar')
  const refresh: (() => void)[] = []
  for (const [label, title, run, active] of buttons) {
    const button = document.createElement('button')
    button.type = 'button'
    button.textContent = label
    button.title = title
    button.addEventListener('mousedown', event => event.preventDefault())
    button.addEventListener('click', run)
    if (active) refresh.push(() => button.setAttribute('aria-pressed', String(active())))
    toolbar.append(button)
  }
  editor.on('transaction', () => refresh.forEach(update => update()))

  // People never see Markdown unless they ask for it: this is that switch.
  const toggle = document.createElement('button')
  toggle.type = 'button'
  toggle.className = 'markdown-toggle'
  toggle.textContent = 'Show Markdown'
  toolbar.append(toggle)

  const panel = el('markdown')
  let pending: number | undefined
  async function loadMarkdown() {
    const response = await fetch(`/api/pages/${encodeURIComponent(pageId)}/markdown`)
    el('markdown-text').textContent = response.ok
      ? await response.text()
      : `Could not load Markdown (${response.status}).`
  }
  toggle.addEventListener('click', () => {
    panel.hidden = !panel.hidden
    toggle.textContent = panel.hidden ? 'Show Markdown' : 'Hide Markdown'
    if (!panel.hidden) void loadMarkdown()
  })
  doc.on('update', () => {
    if (panel.hidden) return
    clearTimeout(pending)
    pending = window.setTimeout(() => void loadMarkdown(), 400)
  })
}
