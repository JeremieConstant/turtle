import type { TokenSummary } from './operations.gen'

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
