/*
 * The editor's own extensions:
 * - Callout: a block Markdown expresses as a GitHub-style alert (> [!NOTE]).
 * - RemoteCarets: other people's cursors and selections. In the real app
 *   CollaborationCaret draws these from the Yjs awareness; here a mock feeds
 *   them in, so the look can be settled before the wiring.
 * - AgentChanges: marks the blocks an Agent just changed, with its name,
 *   then lets the mark fade.
 */
import { Extension, Node, mergeAttributes } from '@tiptap/core'
import { Plugin, PluginKey, type EditorState } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'

export const Callout = Node.create({
  name: 'callout',
  group: 'block',
  content: 'paragraph+',
  defining: true,
  parseHTML: () => [{ tag: 'div[data-callout]' }],
  renderHTML: ({ HTMLAttributes }) => ['div', mergeAttributes(HTMLAttributes, { 'data-callout': 'note' }), 0],
})

/* ---- Other people's cursors ------------------------------------------- */

export type RemoteCaret = { id: string; name: string; color: string; from: number; to: number }
export const remoteCaretsKey = new PluginKey<Map<string, RemoteCaret>>('remoteCarets')

export const RemoteCarets = Extension.create({
  name: 'remoteCarets',
  addProseMirrorPlugins() {
    return [
      new Plugin<Map<string, RemoteCaret>>({
        key: remoteCaretsKey,
        state: {
          init: () => new Map(),
          apply(tr, carets) {
            const next = new Map<string, RemoteCaret>()
            // Keep every cursor where it was as the text around it changes.
            for (const [id, c] of carets) next.set(id, { ...c, from: tr.mapping.map(c.from), to: tr.mapping.map(c.to) })
            const update: RemoteCaret | undefined = tr.getMeta(remoteCaretsKey)
            if (update) next.set(update.id, update)
            return next
          },
        },
        props: {
          decorations(state: EditorState) {
            const carets = remoteCaretsKey.getState(state)
            if (!carets?.size) return null
            const decorations: Decoration[] = []
            for (const c of carets.values()) {
              if (c.to > c.from)
                decorations.push(
                  Decoration.inline(c.from, c.to, {
                    class: 'remote-selection',
                    style: `--person: ${c.color}`,
                  }),
                )
              decorations.push(
                Decoration.widget(c.to, () => caretElement(c), { key: `${c.id}-${c.to}`, side: 1 }),
              )
            }
            return DecorationSet.create(state.doc, decorations)
          },
        },
      }),
    ]
  },
})

function caretElement(c: RemoteCaret) {
  const caret = document.createElement('span')
  caret.className = 'remote-caret'
  caret.style.setProperty('--person', c.color)
  caret.setAttribute('aria-label', `${c.name} is here`)
  const label = document.createElement('span')
  label.className = 'remote-caret-label'
  label.textContent = c.name.split(' ')[0]
  caret.append(label)
  return caret
}

/* ---- An Agent's change -------------------------------------------------- */

export type AgentChange = { id: string; name: string; from: number; to: number; at: number }
export const agentChangesKey = new PluginKey<AgentChange[]>('agentChanges')

export const AgentChanges = Extension.create({
  name: 'agentChanges',
  addProseMirrorPlugins() {
    return [
      new Plugin<AgentChange[]>({
        key: agentChangesKey,
        state: {
          init: () => [],
          apply(tr, changes) {
            let next = changes.map((c) => ({ ...c, from: tr.mapping.map(c.from, 1), to: tr.mapping.map(c.to, -1) }))
            const meta: { add?: AgentChange; remove?: string } | undefined = tr.getMeta(agentChangesKey)
            if (meta?.add) next = [...next, meta.add]
            if (meta?.remove) next = next.filter((c) => c.id !== meta.remove)
            return next.filter((c) => c.to > c.from)
          },
        },
        props: {
          decorations(state) {
            const changes = agentChangesKey.getState(state)
            if (!changes?.length) return null
            const decorations: Decoration[] = []
            for (const c of changes) {
              state.doc.nodesBetween(c.from, c.to, (node, pos) => {
                if (!node.isBlock || node.isTextblock === false) return true
                decorations.push(Decoration.node(pos, pos + node.nodeSize, { class: 'agent-change' }))
                return false
              })
              decorations.push(
                Decoration.widget(c.from + 1, () => agentLabel(c), { key: `agent-${c.id}`, side: -1, ignoreSelection: true }),
              )
            }
            return DecorationSet.create(state.doc, decorations)
          },
        },
      }),
    ]
  },
})

function agentLabel(c: AgentChange) {
  const label = document.createElement('span')
  label.className = 'agent-label'
  label.contentEditable = 'false'
  label.textContent = c.name
  return label
}
