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

/** Where the mark lands when an arrow key moves it through `count` results; past either end it comes round to the other. */
export function moveMark(marked: number, count: number, key: 'ArrowDown' | 'ArrowUp'): number {
  if (count === 0) return marked
  return (marked + (key === 'ArrowDown' ? 1 : count - 1)) % count
}

/**
 * Whether a key press asks for the search field: a slash on its own, while
 * nothing is being typed. With a modifier it belongs to the browser.
 */
export function asksForSearch(
  event: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey'>,
  typing: boolean,
): boolean {
  return event.key === '/' && !typing && !event.metaKey && !event.ctrlKey && !event.altKey
}
