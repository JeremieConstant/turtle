/*
 * Marks the blocks an Agent just changed, with its name, for a moment.
 *
 * Turtle sends an Agent's change as an ordinary update and then announces the
 * Agent's presence; it says nothing about where the change was. So the editor
 * remembers where the last change that arrived over the connection landed,
 * and that is what gets marked when the Agent's announcement follows.
 */
import { Extension } from '@tiptap/core'
import { Plugin, PluginKey, type EditorState, type Transaction } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import { ySyncPluginKey } from '@tiptap/y-tiptap'

/** A marked change: who made it, and where it is now. */
export type AgentChange = { id: string; name: string; from: number; to: number }

type Range = { from: number; to: number }
type State = { arrived: Range | null; changes: AgentChange[] }
type Meta = { add: AgentChange } | { remove: string }

const key = new PluginKey<State>('agentChanges')

export function agentChangesPlugin(): Plugin<State> {
  return new Plugin<State>({
    key,
    state: {
      init: () => ({ arrived: null, changes: [] }),
      apply(tr, { arrived, changes }) {
        // Everything keeps to its text as the text around it changes.
        const map = (range: Range) => ({ from: tr.mapping.map(range.from, 1), to: tr.mapping.map(range.to, -1) })
        let next = changes.map(change => ({ ...change, ...map(change) }))
        if (tr.docChanged) {
          const remote = (tr.getMeta(ySyncPluginKey) as { isChangeOrigin?: boolean } | undefined)?.isChangeOrigin
          arrived = remote ? changedRange(tr) : arrived && map(arrived)
        }
        const meta = tr.getMeta(key) as Meta | undefined
        if (meta && 'add' in meta) next = [...next, meta.add]
        if (meta && 'remove' in meta) next = next.filter(change => change.id !== meta.remove)
        return { arrived: arrived && arrived.to > arrived.from ? arrived : null, changes: next.filter(c => c.to > c.from) }
      },
    },
    props: {
      decorations: state => decorations(state),
    },
  })
}

/** The editor extension around `agentChangesPlugin`. */
export const AgentChanges = Extension.create({
  name: 'agentChanges',
  addProseMirrorPlugins: () => [agentChangesPlugin()],
})

/**
 * Marks the change that arrived last as the named Agent's. Returns the
 * transaction that does it, or null when nothing arrived to mark.
 */
export function markAgentChange(state: EditorState, name: string, id: string): Transaction | null {
  const arrived = key.getState(state)?.arrived
  if (!arrived) return null
  return state.tr.setMeta(key, { add: { id, name, ...arrived } } satisfies Meta).setMeta('addToHistory', false)
}

/** Takes the mark off a change again. */
export function unmarkAgentChange(state: EditorState, id: string): Transaction {
  return state.tr.setMeta(key, { remove: id } satisfies Meta).setMeta('addToHistory', false)
}

/** The changes that are marked now. */
export function agentChanges(state: EditorState): AgentChange[] {
  return key.getState(state)?.changes ?? []
}

/** Where in the document as it is now a transaction changed something. */
function changedRange(tr: Transaction): Range | null {
  let from = Infinity
  let to = -Infinity
  tr.mapping.maps.forEach((map, index) => {
    const after = tr.mapping.slice(index + 1)
    map.forEach((_oldFrom, _oldTo, newFrom, newTo) => {
      from = Math.min(from, after.map(newFrom, -1))
      to = Math.max(to, after.map(newTo, 1))
    })
  })
  return from < to ? { from, to } : null
}

function decorations(state: EditorState): DecorationSet | null {
  const changes = agentChanges(state)
  if (!changes.length) return null
  const found: Decoration[] = []
  for (const change of changes) {
    let labelled = false
    state.doc.forEach((block, offset) => {
      if (offset >= change.to || offset + block.nodeSize <= change.from) return
      found.push(Decoration.node(offset, offset + block.nodeSize, { class: 'agent-change' }))
    })
    // The name goes at the start of the first text the change holds.
    state.doc.nodesBetween(change.from, change.to, (node, pos) => {
      if (labelled || !node.isTextblock) return !labelled
      labelled = true
      found.push(Decoration.widget(pos + 1, () => label(change.name), { key: `agent-${change.id}`, side: -1, ignoreSelection: true }))
      return false
    })
  }
  return DecorationSet.create(state.doc, found)
}

function label(name: string) {
  const element = document.createElement('span')
  element.className = 'agent-label'
  element.contentEditable = 'false'
  element.textContent = name
  return element
}
