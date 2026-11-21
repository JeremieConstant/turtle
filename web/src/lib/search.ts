import type { SearchResult } from './operations.gen'

/**
 * Wraps a search so that only the answer to the latest query counts. Results
 * are shown while typing, and an answer to an earlier keystroke can arrive
 * after a later one; such an answer resolves to `null` instead.
 */
export function latestOnly<A extends unknown[], R>(run: (...args: A) => Promise<R>): (...args: A) => Promise<R | null> {
  let latest = 0
  return async (...args) => {
    const asked = ++latest
    const answer = await run(...args)
    return asked === latest ? answer : null
  }
}

/** Where a result sits: its Area, then the Pages and Folders above it. */
export function place(result: Pick<SearchResult, 'area_name' | 'path'>): string {
  return [result.area_name, ...result.path].join(' › ')
}
