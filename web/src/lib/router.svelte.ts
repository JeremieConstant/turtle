import { parseAddress, type Route } from './routes'

let pathname = $state(location.pathname)

window.addEventListener('popstate', () => {
  pathname = location.pathname
})

/** The screen the address bar points at, and the ways to change it. */
export const router = {
  get route(): Route {
    return parseAddress(pathname)
  },
  /** Goes to another address, keeping the current one in the history. The address is complete, query and fragment included. */
  navigate(address: string) {
    history.pushState(null, '', address)
    pathname = location.pathname
  },
  /**
   * Follows a link inside the app without reloading. A click with a modifier
   * or another button is left to the browser, so it can open a new tab.
   */
  follow(event: MouseEvent, address: string) {
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || event.button !== 0) return
    event.preventDefault()
    this.navigate(address)
  },
  /** Corrects the path of the current address without adding a history entry; its query and fragment stay. */
  replace(address: string) {
    history.replaceState(null, '', address + location.search + location.hash)
    pathname = location.pathname
  },
}
