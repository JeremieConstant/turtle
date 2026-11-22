import { getSchema } from '@tiptap/core'
import { EditorState } from '@tiptap/pm/state'
import StarterKit from '@tiptap/starter-kit'
import { ySyncPluginKey } from '@tiptap/y-tiptap'
import { expect, test } from 'vitest'
import { agentChanges, agentChangesPlugin, beginAgentChange, endAgentChange, unmarkAgentChange } from './agent-changes'

const schema = getSchema([StarterKit])
const paragraph = (text: string) => schema.node('paragraph', null, [schema.text(text)])

/** "One" and "Two": each paragraph takes 5 positions, so "Two" starts at 5. */
function page() {
  return EditorState.create({
    doc: schema.node('doc', null, [paragraph('One'), paragraph('Two')]),
    plugins: [agentChangesPlugin()],
  })
}

/** A change that arrived over the editing connection rather than being typed here. */
function arrive(state: EditorState, at: number, text: string) {
  return state.apply(state.tr.insert(at, paragraph(text)).setMeta(ySyncPluginKey, { isChangeOrigin: true }))
}

/** A change Turtle said is the named Agent's, as the editor applies it. */
function arriveFromAgent(state: EditorState, name: string, id: string, at: number, text: string) {
  const expecting = state.apply(beginAgentChange(state, name, id))
  const changed = arrive(expecting, at, text)
  return changed.apply(endAgentChange(changed))
}

test("an Agent's change is marked with the Agent's name", () => {
  const state = arriveFromAgent(page(), 'Docs Agent', 'a1', 5, 'New')
  expect(agentChanges(state)).toEqual([{ id: 'a1', name: 'Docs Agent', from: 5, to: 10 }])
})

test("a person's change that arrives just before or after an Agent's is not marked", () => {
  let state = arrive(page(), 0, 'Before')
  state = arriveFromAgent(state, 'Docs Agent', 'a1', 18, 'New')
  state = arrive(state, 0, 'After')
  // "Before" (8) and "After" (7) now precede "One" (5) and "Two" (5): "New" starts at 7 + 8 + 5 + 5.
  expect(agentChanges(state)).toEqual([{ id: 'a1', name: 'Docs Agent', from: 25, to: 30 }])
})

test("what is typed here while an Agent's change arrives is not marked", () => {
  let state = page()
  state = state.apply(beginAgentChange(state, 'Docs Agent', 'a1'))
  state = state.apply(state.tr.insert(0, paragraph('Mine')))
  state = state.apply(endAgentChange(state))
  expect(agentChanges(state)).toEqual([])
})

test('the mark stays on the changed text as the text around it changes', () => {
  let state = arriveFromAgent(page(), 'Docs Agent', 'a1', 5, 'New')
  state = state.apply(state.tr.insert(0, paragraph('Zero')))
  expect(agentChanges(state)).toEqual([{ id: 'a1', name: 'Docs Agent', from: 11, to: 16 }])
})

test('the mark goes again after its moment', () => {
  const state = arriveFromAgent(page(), 'Docs Agent', 'a1', 5, 'New')
  expect(agentChanges(state.apply(unmarkAgentChange(state, 'a1')))).toEqual([])
})
