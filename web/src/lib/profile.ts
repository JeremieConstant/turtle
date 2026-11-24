/**
 * The mark that stands in for a person's picture: the first letters of the
 * first and last word of their name. Letters are taken whole, so accented
 * and astral ones survive.
 */
export function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean)
  if (words.length === 0) return '?'
  const first = Array.from(words[0])[0]
  const last = words.length > 1 ? Array.from(words[words.length - 1])[0] : ''
  return (first + last).toLocaleUpperCase()
}

/** What a Profile says about someone's standing: Guest, Workspace Admin, or nothing for a plain Member. */
export function standing(profile: { guest: boolean; workspace_admin: boolean }): 'Guest' | 'Workspace Admin' | null {
  if (profile.guest) return 'Guest'
  if (profile.workspace_admin) return 'Workspace Admin'
  return null
}
