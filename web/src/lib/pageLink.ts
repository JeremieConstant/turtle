import { Node } from '@tiptap/core'
import { Lookup } from './lookup'
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

/**
 * The current titles of the linked Pages on screen. A Page the person cannot
 * read, or that is in the Trash, has no title: `null`.
 */
function titles(): Lookup<string> {
  return new Lookup(async ids => {
    const answer = await call('get_page_titles', { page_ids: ids })
    return new Map(answer.pages.map(page => [page.id, page.title]))
  })
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
    return { titles: titles() }
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
