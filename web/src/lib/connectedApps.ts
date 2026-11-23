import { ask, type Question } from './confirm.svelte'
import type { MembersToken, TokenOwner, TokenSummary } from './operations.gen'
import { OperationError } from './ops'
import { exactTime } from './time'

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

/**
 * What an app or Token may do, in a few words: "Read-only · Only Handbook".
 * `everywhere` says what reaching every Area means for its owner.
 */
export function access(token: TokenSummary, now: number, everywhere = 'All your Areas'): string {
  const reach = token.areas ? `Only ${token.areas.map(area => area.name).join(', ') || 'Areas you can no longer read'}` : everywhere
  const parts = [token.read_only ? 'Read-only' : 'Read and write', reach]
  if (token.expires_at) parts.push(token.expires_at < now ? `Expired ${exactTime(token.expires_at)}` : `Until ${exactTime(token.expires_at)}`)
  return parts.join(' · ')
}

/** Whether a list of apps or Tokens is busy revoking one, and why the last try failed. */
export type Revoking = { busy: boolean; error: string | null }

/**
 * Revokes an app or Token once the Member confirms `question`, keeping
 * `revoking` up to date for the list. Says whether it was revoked.
 */
export async function revokeOnceSure(revoking: Revoking, question: Question, revoke: () => Promise<unknown>): Promise<boolean> {
  if (!(await ask(question))) return false
  revoking.busy = true
  revoking.error = null
  try {
    await revoke()
    return true
  } catch (caught) {
    revoking.error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    return false
  } finally {
    revoking.busy = false
  }
}
