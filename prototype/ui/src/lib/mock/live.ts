/*
 * Stands in for the editing connection (the Yjs WebSocket in the real app)
 * on the open Page: two Members editing with live cursors, and the Docs Agent
 * making one change through an operation (append or replace_section), which
 * arrives with its name and no cursor.
 */
import type { Editor } from '@tiptap/core'
import type { Node as PMNode } from '@tiptap/pm/model'
import { agentChangesKey, remoteCaretsKey } from '../../editor/extensions'
import { DOCS_AGENT, MEMBERS } from './data'
import type { Author } from './types'

export type Editing = { author: Author; color: string | null }

export const EDITING: Editing[] = [
  { author: MEMBERS.lena, color: 'var(--color-person-1)' },
  { author: MEMBERS.tobias, color: 'var(--color-person-2)' },
  { author: DOCS_AGENT, color: null },
]

/** The position just after `needle` in the document, or -1. */
function after(doc: PMNode, needle: string): number {
  let found = -1
  doc.descendants((node, pos) => {
    if (found >= 0) return false
    if (!node.isText) return true
    const i = node.text!.indexOf(needle)
    if (i >= 0) found = pos + i + needle.length
    return false
  })
  return found
}

function before(doc: PMNode, needle: string) {
  const end = after(doc, needle)
  return end < 0 ? -1 : end - needle.length
}

export function startLiveEditing(
  editor: Editor,
  { onAgent }: { onAgent: (author: Author) => void },
): () => void {
  const timers: ReturnType<typeof setTimeout>[] = []
  const later = (ms: number, fn: () => void) => timers.push(setTimeout(fn, ms))
  const alive = () => !editor.isDestroyed

  const setCaret = (who: Editing, from: number, to = from) => {
    if (!alive() || from < 0) return
    const tr = editor.state.tr.setMeta(remoteCaretsKey, {
      id: who.author.id,
      name: who.author.name,
      color: who.color!,
      from,
      to,
    })
    tr.setMeta('addToHistory', false)
    editor.view.dispatch(tr)
  }

  /* Lena types a sentence into "After the deploy". */
  const lena = EDITING[0]
  const sentence = ' The dashboard link is pinned at the top of the deploy channel.'
  setCaret(lena, after(editor.state.doc, 'Check the service dashboard for another fifteen minutes.'))
  let typed = 0
  const typeNext = () => {
    if (!alive()) return
    const carets = remoteCaretsKey.getState(editor.state)
    const at = carets?.get(lena.author.id)?.to
    if (at == null || typed >= sentence.length) return
    const char = sentence[typed++]
    const tr = editor.state.tr.insertText(char, at)
    tr.setMeta('remote', lena.author).setMeta('addToHistory', false)
    editor.view.dispatch(tr)
    const pause = char === ' ' ? 90 + Math.random() * 160 : 45 + Math.random() * 90
    later(typed === 22 ? 1400 : pause, typeNext)
  }
  later(1800, typeNext)

  /* Tobias reads through the table, selecting as he goes. */
  const tobias = EDITING[1]
  const stops = [
    () => {
      const from = before(editor.state.doc, 'p95 latency')
      setCaret(tobias, from, from + 'p95 latency'.length)
    },
    () => setCaret(tobias, after(editor.state.doc, '20 % above baseline')),
    () => setCaret(tobias, after(editor.state.doc, 'More than 2 per pod')),
    () => {
      const from = before(editor.state.doc, 'same hour on the previous three working days')
      setCaret(tobias, from, from + 'same hour on the previous three working days'.length)
    },
  ]
  let stop = 0
  const wander = () => {
    if (!alive()) return
    stops[stop++ % stops.length]()
    later(3200 + Math.random() * 1800, wander)
  }
  wander()

  /* The Docs Agent adds a paragraph to "Running the deploy". */
  later(4200, () => {
    if (!alive()) return
    const anchor = after(editor.state.doc, 'reported in the deploy channel.')
    if (anchor < 0) return
    const $pos = editor.state.doc.resolve(anchor)
    const insertAt = $pos.after($pos.depth)
    const { schema } = editor.state
    const code = schema.marks.code.create()
    const paragraph = schema.nodes.paragraph.create(null, [
      schema.text('Since version 2.4 of the platform CLI, '),
      schema.text('plat deploy', [code]),
      schema.text(' refuses an image that has not run on staging. Pass '),
      schema.text('--allow-unstaged', [code]),
      schema.text(' only during an incident, and say so in the incident channel.'),
    ])
    const tr = editor.state.tr.insert(insertAt, paragraph)
    tr.setMeta('remote', DOCS_AGENT).setMeta('addToHistory', false)
    const change = { id: `change-${Date.now()}`, name: DOCS_AGENT.name, from: insertAt, to: insertAt + paragraph.nodeSize, at: Date.now() }
    tr.setMeta(agentChangesKey, { add: change })
    editor.view.dispatch(tr)
    onAgent(DOCS_AGENT)
    // The mark has faded by now (tokens.css: 5s hold plus --dur-agent); drop it.
    later(8000, () => alive() && editor.view.dispatch(editor.state.tr.setMeta(agentChangesKey, { remove: change.id })))
  })

  return () => timers.forEach(clearTimeout)
}
