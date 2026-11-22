import { getSchema } from '@tiptap/core'
import { EditorState } from '@tiptap/pm/state'
import StarterKit from '@tiptap/starter-kit'
import { ySyncPluginKey } from '@tiptap/y-tiptap'
import { expect, test } from 'vitest'
import { agentChanges, agentChangesPlugin, markAgentChange, unmarkAgentChange } from './agent-changes'

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

test("the change that just arrived is marked with the Agent's name", () => {
  const state = arrive(page(), 5, 'New')
  const marked = state.apply(markAgentChange(state, 'Docs Agent', 'a1')!)
  expect(agentChanges(marked)).toEqual([{ id: 'a1', name: 'Docs Agent', from: 5, to: 10 }])
})

test('what was typed here is never taken for an Agent\'s change', () => {
  const state = page()
  const typed = state.apply(state.tr.insert(5, paragraph('Mine')))
  expect(markAgentChange(typed, 'Docs Agent', 'a1')).toBeNull()
})

test('the mark stays on the changed text as the text around it changes', () => {
  const state = arrive(page(), 5, 'New')
  let marked = state.apply(markAgentChange(state, 'Docs Agent', 'a1')!)
  marked = marked.apply(marked.tr.insert(0, paragraph('Zero')))
  expect(agentChanges(marked)).toEqual([{ id: 'a1', name: 'Docs Agent', from: 11, to: 16 }])
})

test('the mark goes again after its moment', () => {
  const state = arrive(page(), 5, 'New')
  const marked = state.apply(markAgentChange(state, 'Docs Agent', 'a1')!)
  expect(agentChanges(marked.apply(unmarkAgentChange(marked, 'a1')))).toEqual([])
})
