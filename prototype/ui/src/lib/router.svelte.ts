import { parseAddress, type Route } from './routes'

let location_ = $state({ pathname: location.pathname, search: location.search })
const sync = () => (location_ = { pathname: location.pathname, search: location.search })
window.addEventListener('popstate', sync)

/** The screen the address bar points at, and the ways to change it. */
export const router = {
  get route(): Route {
    return parseAddress(location_.pathname, location_.search)
  },
  get pathname() {
    return location_.pathname
  },
  navigate(address: string) {
    history.pushState(null, '', address)
    sync()
  },
  replace(address: string) {
    history.replaceState(null, '', address)
    sync()
  },
  /** Follows a link without reloading; a modified click is left to the browser. */
  follow(event: MouseEvent, address: string) {
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || event.button !== 0) return
    event.preventDefault()
    this.navigate(address)
  },
}
