/** What a node view learns about the thing it shows: the answer, `null` when there is none for this person, or `undefined` when asking failed. */
export type Shown<T> = (answer: T | null | undefined) => void

/**
 * What the Page links or Mentions on screen show, asked for together by ID.
 * Answers are asked for again whenever the window comes back into view, so a
 * rename or lost access shows without reopening the Page.
 */
export class Lookup<T> {
  private shown = new Map<string, Set<Shown<T>>>()
  private known = new Map<string, T | null>()
  private unasked = new Set<string>()
  private asking: ReturnType<typeof setTimeout> | undefined
  private listening = false

  /** `fetch` answers for the IDs it knows; an ID it leaves out has no answer. */
  constructor(private fetch: (ids: string[]) => Promise<Map<string, T>>) {}

  /** Shows the answer through `show` now and whenever it changes, until the returned function is called. */
  watch(id: string, show: Shown<T>): () => void {
    // Only once something is shown, so building the editor's schema alone needs no window.
    if (!this.listening) {
      document.addEventListener('visibilitychange', this.refresh)
      window.addEventListener('focus', this.refresh)
      this.listening = true
    }
    const watchers = this.shown.get(id) ?? new Set()
    watchers.add(show)
    this.shown.set(id, watchers)
    if (this.known.has(id)) show(this.known.get(id))
    else this.ask([id])
    return () => {
      watchers.delete(show)
      if (watchers.size === 0) this.shown.delete(id)
    }
  }

  stop() {
    if (this.listening) {
      document.removeEventListener('visibilitychange', this.refresh)
      window.removeEventListener('focus', this.refresh)
      this.listening = false
    }
    clearTimeout(this.asking)
  }

  private refresh = () => {
    if (document.visibilityState === 'visible') this.ask([...this.shown.keys()])
  }

  /** What a Page holds appears together, so it is asked for in one call. */
  private ask(ids: string[]) {
    ids.forEach(id => this.unasked.add(id))
    clearTimeout(this.asking)
    this.asking = setTimeout(() => void this.send(), 0)
  }

  private async send() {
    const ids = [...this.unasked]
    this.unasked.clear()
    if (ids.length === 0) return
    let answers: Map<string, T> | undefined
    try {
      answers = await this.fetch(ids)
    } catch {
      answers = undefined
    }
    for (const id of ids) {
      // A failure is not remembered: what was known stays, and the next refresh asks again.
      const answer = answers ? (answers.get(id) ?? null) : this.known.has(id) ? this.known.get(id) : undefined
      if (answers) this.known.set(id, answer ?? null)
      this.shown.get(id)?.forEach(show => show(answer))
    }
  }
}
