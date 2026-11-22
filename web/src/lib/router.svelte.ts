import { parseAddress, type Route } from './routes'

/** The parts of the address a screen is chosen by. */
const read = () => ({ pathname: location.pathname, query: location.search })

let address = $state.raw(read())

window.addEventListener('popstate', () => {
  address = read()
})

/** The screen the address bar points at, and the ways to change it. */
export const router = {
  get route(): Route {
    return parseAddress(address.pathname, address.query)
  },
  /** Goes to another address, keeping the current one in the history. The address is complete, query and fragment included. */
  navigate(to: string) {
    history.pushState(null, '', to)
    address = read()
  },
  /**
   * Follows a link inside the app without reloading. A click with a modifier
   * or another button is left to the browser, so it can open a new tab.
   */
  follow(event: MouseEvent, to: string) {
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || event.button !== 0) return
    event.preventDefault()
    this.navigate(to)
  },
  /** Corrects the path of the current address without adding a history entry; its query and fragment stay. */
  replace(path: string) {
    history.replaceState(null, '', path + location.search + location.hash)
    address = read()
  },
  /**
   * Puts a complete address, query included, in place of the current one
   * without adding a history entry. A query that is being typed uses it, so
   * going back leaves the search instead of removing one letter.
   */
  rewrite(to: string) {
    history.replaceState(null, '', to)
    address = read()
  },
}
