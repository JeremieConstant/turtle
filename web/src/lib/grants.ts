import type { GrantHolder, GrantList } from './operations.gen'

/** A holder as the value of a choice in a form. */
export function holderKey(holder: GrantHolder): string {
  switch (holder.kind) {
    case 'all_members':
      return 'all_members'
    case 'member':
      return `member:${holder.member_id}`
    case 'agent':
      return `agent:${holder.agent_id}`
  }
}

export function holderFromKey(key: string): GrantHolder {
  if (key === 'all_members') return { kind: 'all_members' }
  if (key.startsWith('agent:')) return { kind: 'agent', agent_id: key.slice('agent:'.length) }
  return { kind: 'member', member_id: key.slice('member:'.length) }
}

/** How a holder is named in the list, an Agent always recognisable as one. */
export function holderLabel(holder: GrantHolder, name: string): string {
  return holder.kind === 'agent' ? `${name} (Agent)` : name
}

/** Whom a Role can be given to that holds no Grant on the Area yet: a key and what to show. */
export function withoutGrant(list: GrantList): { key: string; label: string }[] {
  const granted = new Set(list.grants.map(grant => holderKey(grant.holder)))
  const choice = (holder: GrantHolder, name: string) => ({ key: holderKey(holder), label: holderLabel(holder, name) })
  const choices = [
    choice({ kind: 'all_members' }, 'All Members'),
    ...list.members.map(member => choice({ kind: 'member', member_id: member.id }, member.name)),
    ...list.agents.map(agent => choice({ kind: 'agent', agent_id: agent.id }, agent.name)),
  ]
  return choices.filter(choice => !granted.has(choice.key))
}
