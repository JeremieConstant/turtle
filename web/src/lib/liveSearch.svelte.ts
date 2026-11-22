import type { SearchResult } from './operations.gen'
import { call, OperationError } from './ops'
import { latestOnly } from './search'

/** How long typing must pause before a search is sent, in milliseconds. */
const PAUSE = 120

/**
 * Results that follow a query while it is typed, in one Area or everywhere
 * the person can read. Call it while a component is being set up; it searches
 * for as long as that component is on the screen.
 */
export function liveSearch(asked: () => { query: string; areaId?: string; limit?: number }) {
  let results = $state.raw<SearchResult[] | null>(null)
  let error = $state<string | null>(null)
  /** The query the results answer, which is behind the one being typed until its answer arrives. */
  let answered = $state('')
  let more = $state(false)

  const ask = latestOnly((query: string, area_id?: string, limit?: number) =>
    call('search', { query, area_id, limit }),
  )
  const text = () => asked().query.trim()

  $effect(() => {
    const query = text()
    const { areaId, limit } = asked()
    if (!query) {
      results = null
      error = null
      return
    }
    // Set once something else is asked or the field is emptied; whatever comes back after that is late.
    let left = false
    const timer = setTimeout(async () => {
      try {
        const answer = await ask(query, areaId, limit)
        if (!answer || left) return
        results = answer.results
        more = answer.more
        answered = query
        error = null
      } catch (caught) {
        // A late failure must not cover the results of what was asked since.
        if (left) return
        error = caught instanceof OperationError ? caught.message : 'Searching failed. Try again.'
      }
    }, PAUSE)
    return () => {
      left = true
      clearTimeout(timer)
    }
  })

  return {
    /** The results for `answered`, best first; `null` while there is no query or no answer yet. */
    get results() {
      return results
    },
    get answered() {
      return answered
    },
    /** Whether more Pages matched than the limit let through. */
    get more() {
      return more
    },
    /** What to tell the person when the last search failed. */
    get error() {
      return error
    },
  }
}
