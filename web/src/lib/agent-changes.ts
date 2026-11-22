/*
 * Marks the blocks an Agent just changed, with its name, for a moment.
 *
 * Turtle sends an Agent's change in a frame of its own that names the Agent.
 * The editor applies it between `beginAgentChange` and `endAgentChange`, and
 * the change that arrives over the connection in between is marked: exactly
 * that one, never a person's change that arrives just before or after.
 */
import { Extension } from '@tiptap/core'
import { Plugin, PluginKey, type EditorState, type Transaction } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import { ySyncPluginKey } from '@tiptap/y-tiptap'

/** A marked change: who made it, and where it is now. */
export type AgentChange = { id: string; name: string; from: number; to: number }

type Range = { from: number; to: number }
/** The Agent whose change is being applied right now, if any, and what is marked. */
type State = { expecting: { id: string; name: string } | null; changes: AgentChange[] }
type Meta = { begin: { id: string; name: string } } | { end: true } | { remove: string }

const key = new PluginKey<State>('agentChanges')

export function agentChangesPlugin(): Plugin<State> {
  return new Plugin<State>({
    key,
    state: {
      init: () => ({ expecting: null, changes: [] }),
      apply(tr, { expecting, changes }) {
        // Every mark keeps to its text as the text around it changes.
        let next = changes.map(change => ({
          ...change,
          from: tr.mapping.map(change.from, 1),
          to: tr.mapping.map(change.to, -1),
        }))
        const remote = (tr.getMeta(ySyncPluginKey) as { isChangeOrigin?: boolean } | undefined)?.isChangeOrigin
        const agent = expecting
        const range = agent && tr.docChanged && remote ? changedRange(tr) : null
        if (agent && range) {
          // One change of an Agent may arrive in more than one step; it stays one mark.
          const known = next.find(change => change.id === agent.id)
          next = known
            ? next.map(c => (c === known ? { ...c, from: Math.min(c.from, range.from), to: Math.max(c.to, range.to) } : c))
            : [...next, { ...agent, ...range }]
        }
        const meta = tr.getMeta(key) as Meta | undefined
        if (meta && 'begin' in meta) expecting = meta.begin
        if (meta && 'end' in meta) expecting = null
        if (meta && 'remove' in meta) next = next.filter(change => change.id !== meta.remove)
        return { expecting, changes: next.filter(change => change.to > change.from) }
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

/** Says that the change arriving next over the connection is the named Agent's; `id` names its mark. */
export function beginAgentChange(state: EditorState, name: string, id: string): Transaction {
  return state.tr.setMeta(key, { begin: { id, name } } satisfies Meta).setMeta('addToHistory', false)
}

/** Says that the Agent's change has arrived; what arrives next is nobody's in particular again. */
export function endAgentChange(state: EditorState): Transaction {
  return state.tr.setMeta(key, { end: true } satisfies Meta).setMeta('addToHistory', false)
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
