/** What an address in the address bar points at (ADR-0007). */
export type Route =
  | { screen: 'home' }
  | { screen: 'setup' }
  | { screen: 'sign-in' }
  | { screen: 'area'; id: string }
  | { screen: 'page'; id: string }
  | { screen: 'not-found' }

export function parseAddress(pathname: string): Route {
  if (pathname === '/') return { screen: 'home' }
  if (pathname === '/setup') return { screen: 'setup' }
  if (pathname === '/sign-in') return { screen: 'sign-in' }
  // The readable part is only decoration, so it is matched and ignored.
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

/** The address of a Page: its ID, then its title in a readable form. */
export function pageAddress(id: string, title: string): string {
  return address('p', id, title)
}

function address(prefix: 'a' | 'p', id: string, label: string): string {
  const readable = label
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-|-$/g, '')
  const fixed = `/${prefix}/${encodeURIComponent(id)}`
  return readable ? `${fixed}/${encodeURIComponent(readable)}` : fixed
}
