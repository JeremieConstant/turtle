import { call } from './ops'
import type { Member } from './operations.gen'

let loaded = $state(false)
let setupRequired = $state(false)
let member = $state.raw<Member | null>(null)

/** Who is signed in, and whether the installation still needs its first account. */
export const session = {
  get loaded() {
    return loaded
  },
  get setupRequired() {
    return setupRequired
  },
  get member() {
    return member
  },
  /** Asks the server. Called once on start; the operations below keep it current afterwards. */
  async load() {
    const current = await call('get_session', {})
    setupRequired = current.setup_required
    member = current.member ?? null
    loaded = true
  },
  /** Takes in the Member an operation has just signed in. */
  signedIn(newcomer: Member) {
    setupRequired = false
    member = newcomer
  },
  async signOut() {
    await call('sign_out', {})
    member = null
  },
}
