/** What an address in the address bar points at (ADR-0007). */
export type Route =
  | { screen: 'home' }
  | { screen: 'setup' }
  | { screen: 'sign-in' }
  | { screen: 'area'; id: string }
  | { screen: 'page'; id: string }
  | { screen: 'members' }
  | { screen: 'tokens' }
  | { screen: 'agents' }
  /** Where an invitation link leads; the secret is the link's last part. */
  | { screen: 'invitation'; secret: string }
  /** Where a password reset link leads. */
  | { screen: 'password-reset'; secret: string }
  | { screen: 'not-found' }

export function parseAddress(pathname: string): Route {
  if (pathname === '/') return { screen: 'home' }
  if (pathname === '/setup') return { screen: 'setup' }
  if (pathname === '/sign-in') return { screen: 'sign-in' }
  if (pathname === '/settings/members') return { screen: 'members' }
  if (pathname === '/settings/tokens') return { screen: 'tokens' }
  if (pathname === '/settings/agents') return { screen: 'agents' }
  const link = pathname.match(/^\/(join|reset-password)\/([^/]+)$/)
  if (link) {
    return link[1] === 'join' ? { screen: 'invitation', secret: link[2] } : { screen: 'password-reset', secret: link[2] }
  }
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
