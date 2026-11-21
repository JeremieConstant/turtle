import type { Author, VersionSummary } from './operations.gen'

/** The authors of a Version as a sentence lists them: "Ada, Grace and Linus". */
export function authorNames(authors: Author[]): string {
  const names = authors.map(author => author.name)
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
