import { describe, expect, it } from 'vitest'
import { answer, ask, confirmation } from './confirm.svelte'

const QUESTION = { title: 'Revoke CI pipeline?', message: 'Agents using it lose access at once.', action: 'Revoke' }

describe('ask', () => {
  it('shows the question and says yes once it is confirmed', async () => {
    const asked = ask(QUESTION)
    expect(confirmation.question).toEqual(QUESTION)
    answer(true)
    await expect(asked).resolves.toBe(true)
    expect(confirmation.question).toBeNull()
  })

  it('says no once it is cancelled', async () => {
    const asked = ask(QUESTION)
    answer(false)
    await expect(asked).resolves.toBe(false)
  })

  it('takes a new question for no to the one still open', async () => {
    const first = ask(QUESTION)
    const second = ask({ ...QUESTION, title: 'Revoke Laptop?' })
    await expect(first).resolves.toBe(false)
    expect(confirmation.question?.title).toBe('Revoke Laptop?')
    answer(true)
    await expect(second).resolves.toBe(true)
  })
})
