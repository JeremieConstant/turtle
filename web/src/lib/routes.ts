/** What an address in the address bar points at (ADR-0007). */
export type Route =
  | { screen: 'home' }
  | { screen: 'setup' }
  | { screen: 'sign-in' }
  | { screen: 'area'; id: string }
  | { screen: 'page'; id: string }
  /** The Trash of an Area. */
  | { screen: 'trash'; areaId: string }
  /** Every result for a query. The query is part of the address, so a search can be shared. */
  | { screen: 'search'; query: string }
  | { screen: 'members' }
  /** How to connect an AI client to Turtle. */
  | { screen: 'connect-ai' }
  /** Everything that can act as the signed-in Member: connected apps and Tokens made by hand. */
  | { screen: 'connected-apps' }
  /** Every Member's connected apps and Tokens, for a Workspace Admin to see and disconnect. */
  | { screen: 'all-connected-apps' }
  | { screen: 'agents' }
  /** The PDF Styles, for a Workspace Admin to manage. */
  | { screen: 'pdf-styles' }
  /** The deleted Areas, for a Workspace Admin to restore. */
  | { screen: 'deleted-areas' }
  /** Where an invitation link leads; the secret is the link's last part. */
  | { screen: 'invitation'; secret: string }
  /** Where a password reset link leads. */
  | { screen: 'password-reset'; secret: string }
  /** Where an app asking to connect sends the browser, with its OAuth request as the query (with the question mark). */
  | { screen: 'authorize'; query: string }
  | { screen: 'not-found' }

/** Reads an address: its path, and its query part as the address bar holds it, with the question mark. */
export function parseAddress(pathname: string, query = ''): Route {
  if (pathname === '/') return { screen: 'home' }
  if (pathname === '/search') return { screen: 'search', query: new URLSearchParams(query).get('q') ?? '' }
  if (pathname === '/setup') return { screen: 'setup' }
  if (pathname === '/sign-in') return { screen: 'sign-in' }
  if (pathname === '/authorize') return { screen: 'authorize', query }
  if (pathname === '/settings/connect-ai') return { screen: 'connect-ai' }
  if (pathname === '/settings/members') return { screen: 'members' }
  if (pathname === '/settings/connected-apps') return { screen: 'connected-apps' }
  if (pathname === '/settings/all-connected-apps') return { screen: 'all-connected-apps' }
  if (pathname === '/settings/agents') return { screen: 'agents' }
  if (pathname === '/settings/pdf-styles') return { screen: 'pdf-styles' }
  if (pathname === '/settings/deleted-areas') return { screen: 'deleted-areas' }
  const link = pathname.match(/^\/(join|reset-password)\/([^/]+)$/)
  if (link) {
    return link[1] === 'join' ? { screen: 'invitation', secret: link[2] } : { screen: 'password-reset', secret: link[2] }
  }
  // The readable part is only decoration, so it is matched and ignored.
  const trash = pathname.match(/^\/a\/([^/]+)\/(?:[^/]*\/)?trash$/)
  if (trash) {
    try {
      return { screen: 'trash', areaId: decodeURIComponent(trash[1]) }
    } catch {
      return { screen: 'not-found' }
    }
  }
  const addressed = pathname.match(/^\/([ap])\/([^/]+)(?:\/[^/]*)?\/?$/)
  if (addressed) {
    try {
      const id = decodeURIComponent(addressed[2])
      return addressed[1] === 'a' ? { screen: 'area', id } : { screen: 'page', id }
    } catch {
      // A broken percent-encoding names nothing.
    }
  }
  return { screen: 'not-found' }
}

/** The address of an Area: its ID, then its name in a readable form. */
export function areaAddress(id: string, name: string): string {
  return address('a', id, name)
}

/** The address of the Trash of an Area: the Area's, then "trash". */
export function trashAddress(areaId: string, areaName: string): string {
  const area = areaAddress(areaId, areaName)
  return area.split('/').length > 3 ? `${area}/trash` : `${area}//trash`
}

/** The address of a Page: its ID, then its title in a readable form. */
export function pageAddress(id: string, title: string): string {
  return address('p', id, title)
}

/** The address that shows every result for a query. */
export function searchAddress(query: string): string {
  return query ? `/search?q=${encodeURIComponent(query)}` : '/search'
}

/** The address of the search for the Pages that carry a Tag. */
export function tagSearchAddress(tag: string): string {
  return searchAddress(`#${tag}`)
}

function address(prefix: 'a' | 'p', id: string, label: string): string {
  const readable = label
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-|-$/g, '')
  const fixed = `/${prefix}/${encodeURIComponent(id)}`
  return readable ? `${fixed}/${encodeURIComponent(readable)}` : fixed
}

/** The screens that have no place in the shell: nobody is signed in on them, or need not be. */
const OUTSIDE_SHELL: Route['screen'][] = ['setup', 'sign-in', 'invitation', 'password-reset', 'authorize']

/** Whether a screen sits in the shell, beside the sidebar. */
export function inShell(route: Route): boolean {
  return !OUTSIDE_SHELL.includes(route.screen)
}
