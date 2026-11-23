import type { MembersToken, TokenOwner, TokenSummary } from './operations.gen'

/** One Member's apps and Tokens, as a Workspace Admin sees them. */
export type MembersApps = { member: TokenOwner; apps: TokenSummary[] }

/** Every Member's apps and Tokens, under the Member, keeping the order they came in. */
export function byMember(entries: MembersToken[]): MembersApps[] {
  const grouped: MembersApps[] = []
  for (const { member, token } of entries) {
    const last = grouped.at(-1)
    if (last?.member.id === member.id) last.apps.push(token)
    else grouped.push({ member, apps: [token] })
  }
  return grouped
}

/** A moment in seconds since the Unix epoch, as the reader's locale writes it. */
export function when(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString()
}

/**
 * What an app or Token may do, in a few words: "Read-only · Only Handbook".
 * `everywhere` says what reaching every Area means for its owner.
 */
export function access(token: TokenSummary, now: number, everywhere = 'All your Areas'): string {
  const reach = token.areas ? `Only ${token.areas.map(area => area.name).join(', ') || 'Areas you can no longer read'}` : everywhere
  const parts = [token.read_only ? 'Read-only' : 'Read and write', reach]
  if (token.expires_at) parts.push(token.expires_at < now ? `Expired ${when(token.expires_at)}` : `Until ${when(token.expires_at)}`)
  return parts.join(' · ')
}
