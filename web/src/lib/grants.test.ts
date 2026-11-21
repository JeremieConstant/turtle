import { describe, expect, it } from 'vitest'
import { holderFromKey, holderKey, withoutGrant } from './grants'
import type { GrantList } from './operations.gen'

describe('holders in a choice', () => {
  it('turn into a key and back', () => {
    expect(holderFromKey(holderKey({ kind: 'member', member_id: 'ab12' }))).toEqual({ kind: 'member', member_id: 'ab12' })
    expect(holderFromKey(holderKey({ kind: 'all_members' }))).toEqual({ kind: 'all_members' })
    expect(holderFromKey(holderKey({ kind: 'agent', agent_id: 'r1' }))).toEqual({ kind: 'agent', agent_id: 'r1' })
  })
})

describe('whom a Role can still be given to', () => {
  const list: GrantList = {
    area: { id: 'a1', name: 'Engineering', role: 'admin' },
    grants: [{ holder: { kind: 'member', member_id: 'grace' }, name: 'Grace', role: 'admin' }],
    members: [
      { id: 'grace', name: 'Grace' },
      { id: 'ken', name: 'Ken' },
    ],
    agents: [{ id: 'r1', name: 'Reviewer' }],
  }

  it('is all Members, then every Member and every Agent without a Grant yet', () => {
    expect(withoutGrant(list)).toEqual([
      { key: 'all_members', label: 'All Members' },
      { key: 'member:ken', label: 'Ken' },
      { key: 'agent:r1', label: 'Reviewer (Agent)' },
    ])
  })

  it('leaves out all Members once they hold a Grant', () => {
    const shared = { ...list, grants: [...list.grants, { holder: { kind: 'all_members' as const }, name: 'All Members', role: 'read' as const }] }
    expect(withoutGrant(shared)).toEqual([
      { key: 'member:ken', label: 'Ken' },
      { key: 'agent:r1', label: 'Reviewer (Agent)' },
    ])
  })
})
