import type { Author, VersionSummary } from './operations.gen'

/**
 * The authors of a Version as a sentence lists them, every Agent recognisable
 * as one: "Ada, Claude (Agent of Linus) and Reviewer (Agent)".
 */
export function authorNames(authors: Author[]): string {
  const names = authors.map(authorName)
  if (names.length === 0) return 'Unknown'
  if (names.length === 1) return names[0]
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
}

/** What a Version is called in the list, saying which one a restore brought back. */
export function versionTitle(version: Pick<VersionSummary, 'number' | 'restored_from'>): string {
  const title = `Version ${version.number}`
  return version.restored_from == null ? title : `${title}, restored from Version ${version.restored_from}`
}

/** The number of the Version recorded just before the given one, in a list that is newest first. */
export function previousNumber(versions: Pick<VersionSummary, 'number'>[], number: number): number | null {
  const index = versions.findIndex(version => version.number === number)
  return index < 0 ? null : (versions[index + 1]?.number ?? null)
}

function authorName(author: Author): string {
  switch (author.kind) {
    case 'member':
      return author.name
    case 'agent_for_member':
      return `${author.name} (Agent of ${author.member_name})`
    case 'agent':
      return `${author.name} (Agent)`
  }
}
