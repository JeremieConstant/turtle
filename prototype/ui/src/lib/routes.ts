/* The addresses of ADR-0007: the title part is only for reading. */
export type Route =
  | { screen: 'start' }
  | { screen: 'area'; area_id: string }
  | { screen: 'page'; page_id: string }
  | { screen: 'search'; query: string }
  | { screen: 'not-found' }

export const slug = (text: string) =>
  text
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')

export const areaAddress = (id: string, name: string) => `/a/${id}/${slug(name)}`
export const pageAddress = (id: string, title: string) => `/p/${id}/${slug(title)}`
export const searchAddress = (query: string) => `/search${query ? `?q=${encodeURIComponent(query)}` : ''}`

export function parseAddress(pathname: string, search: string): Route {
  const parts = pathname.split('/').filter(Boolean)
  if (parts.length === 0) return { screen: 'start' }
  if (parts[0] === 'a' && parts[1]) return { screen: 'area', area_id: parts[1] }
  if (parts[0] === 'p' && parts[1]) return { screen: 'page', page_id: parts[1] }
  if (parts[0] === 'search') return { screen: 'search', query: new URLSearchParams(search).get('q') ?? '' }
  return { screen: 'not-found' }
}
