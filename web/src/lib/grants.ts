import type { GrantHolder, GrantList } from './operations.gen'

/** A holder as the value of a choice in a form. */
export function holderKey(holder: GrantHolder): string {
  return holder.kind === 'all_members' ? 'all_members' : `member:${holder.member_id}`
}

export function holderFromKey(key: string): GrantHolder {
  return key === 'all_members' ? { kind: 'all_members' } : { kind: 'member', member_id: key.slice('member:'.length) }
}

/** Whom a Role can be given to that holds no Grant on the Area yet: a key and what to show. */
export function withoutGrant(list: GrantList): { key: string; label: string }[] {
  const granted = new Set(list.grants.map(grant => holderKey(grant.holder)))
  const choices = [
    { key: holderKey({ kind: 'all_members' }), label: 'All Members' },
    ...list.members.map(member => ({
      key: holderKey({ kind: 'member', member_id: member.id }),
      label: member.name,
    })),
  ]
  return choices.filter(choice => !granted.has(choice.key))
}
