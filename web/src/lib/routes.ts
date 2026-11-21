/** What an address in the address bar points at (ADR-0007). */
export type Route =
  | { screen: 'home' }
  | { screen: 'setup' }
  | { screen: 'sign-in' }
  | { screen: 'page'; id: string }
  | { screen: 'not-found' }

export function parseAddress(pathname: string): Route {
  if (pathname === '/') return { screen: 'home' }
  if (pathname === '/setup') return { screen: 'setup' }
  if (pathname === '/sign-in') return { screen: 'sign-in' }
  // The title part is only for readability, so it is matched and ignored.
  const page = pathname.match(/^\/p\/([^/]+)(?:\/[^/]*)?\/?$/)
  if (page) {
    try {
      return { screen: 'page', id: decodeURIComponent(page[1]) }
    } catch {
      // A broken percent-encoding names no Page.
    }
  }
  return { screen: 'not-found' }
}

/** The address of a Page: its ID, then its title in a readable form. */
export function pageAddress(id: string, title: string): string {
  const readable = title
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-|-$/g, '')
  const address = `/p/${encodeURIComponent(id)}`
  return readable ? `${address}/${encodeURIComponent(readable)}` : address
}
