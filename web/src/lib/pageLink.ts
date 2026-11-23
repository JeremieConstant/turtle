import { Node } from '@tiptap/core'
import { call } from './ops'
import { router } from './router.svelte'
import { pageAddress } from './routes'

/** How a Page link is written as a link address: `turtle:` and the Page's ID, as in its Markdown. */
export const SCHEME = 'turtle:'

/** The ID of the Page a link address points at, when it is a Page link; as the server reads it. */
export function pageIdOf(address: string): string | null {
  const id = address.startsWith(SCHEME) ? address.slice(SCHEME.length) : ''
  return /^[A-Za-z0-9_-]+$/.test(id) ? id : null
}

/** What a Page link says when it may not show its Page's title. */
export const UNAVAILABLE = 'Page not available'

/** What a Page link learns about its Page: its title, `null` for no title to show, or `undefined` when asking failed. */
type Shown = (title: string | null | undefined) => void

/**
 * The current titles of the linked Pages on screen, asked for together. A
 * Page the person cannot read, or that is in the Trash, has no title: `null`.
 * They are asked for again whenever the window comes back into view, so a
 * rename, a move to the Trash or lost access shows without reopening the Page.
 */
class Titles {
  private shown = new Map<string, Set<Shown>>()
  private known = new Map<string, string | null>()
  private unasked = new Set<string>()
  private asking: ReturnType<typeof setTimeout> | undefined
  private listening = false

  /** Shows the title through `show` now and whenever it changes, until the returned function is called. */
  watch(pageId: string, show: Shown): () => void {
    // Only once a link is shown, so building the editor's schema alone needs no window.
    if (!this.listening) {
      document.addEventListener('visibilitychange', this.refresh)
      window.addEventListener('focus', this.refresh)
      this.listening = true
    }
    const watchers = this.shown.get(pageId) ?? new Set()
    watchers.add(show)
    this.shown.set(pageId, watchers)
    if (this.known.has(pageId)) show(this.known.get(pageId))
    else this.ask([pageId])
    return () => {
      watchers.delete(show)
      if (watchers.size === 0) this.shown.delete(pageId)
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

  /** The links of a Page appear together, so they are asked for in one call. */
  private ask(ids: string[]) {
    ids.forEach(id => this.unasked.add(id))
    clearTimeout(this.asking)
    this.asking = setTimeout(() => void this.send(), 0)
  }

  private async send() {
    const ids = [...this.unasked]
    this.unasked.clear()
    if (ids.length === 0) return
    let titles: Map<string, string> | undefined
    try {
      const answer = await call('get_page_titles', { page_ids: ids })
      titles = new Map(answer.pages.map(page => [page.id, page.title]))
    } catch {
      titles = undefined
    }
    for (const id of ids) {
      // A failure is not remembered: what was known stays, and the next refresh asks again.
      const title = titles ? (titles.get(id) ?? null) : this.known.has(id) ? this.known.get(id) : undefined
      if (titles) this.known.set(id, title ?? null)
      this.shown.get(id)?.forEach(show => show(title))
    }
  }
}

/**
 * A link to another Page by its ID. It shows the Page's current title, so
 * renaming or moving the Page never breaks it, and to someone who may not
 * read the Page it shows neither title nor a way to follow it.
 */
export const PageLink = Node.create({
  name: 'pageLink',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  // The Page's title is its whole text, so it carries no formatting (as in Markdown).
  marks: '',

  addStorage() {
    // One per editor, so opening a Page asks for its titles afresh.
    return { titles: new Titles() }
  },

  onDestroy() {
    this.storage.titles.stop()
  },

  addAttributes() {
    return {
      pageId: {
        default: null,
        parseHTML: element => pageIdOf(element.getAttribute('href') ?? ''),
        renderHTML: attributes => ({ href: `${SCHEME}${attributes.pageId}` }),
      },
    }
  },

  parseHTML() {
    return [{ tag: `a[href^="${SCHEME}"]` }]
  },

  renderHTML({ HTMLAttributes }) {
    return ['a', { ...HTMLAttributes, 'data-page-link': '' }]
  },

  addNodeView() {
    return ({ node }) => {
      const pageId = String(node.attrs.pageId)
      const element = document.createElement('a')
      element.classList.add('page-link', 'loading')
      element.dataset.pageLink = ''
      element.textContent = '…'
      let address: string | null = null
      // A Page link is followed with a click, while editing too: being one piece, it is
      // selected with the keyboard instead.
      element.addEventListener('click', event => {
        if (address) router.follow(event, address)
        else event.preventDefault()
      })
      const stop = this.storage.titles.watch(pageId, title => {
        element.classList.toggle('loading', false)
        element.classList.toggle('unavailable', !title)
        if (!title) {
          address = null
          element.removeAttribute('href')
          element.textContent = UNAVAILABLE
          element.title =
            title === null
              ? 'This Page is not available to you.'
              : 'The title of this Page could not be loaded. Check your connection; it is tried again when you come back to this window.'
          return
        }
        address = pageAddress(pageId, title)
        element.href = address
        element.removeAttribute('title')
        element.textContent = title
      })
      return { dom: element, destroy: stop }
    }
  },
})
