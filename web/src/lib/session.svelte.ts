import { call } from './ops'
import type { Demo, Member } from './operations.gen'

let loaded = $state(false)
let setupRequired = $state(false)
let sendsMail = $state(false)
let member = $state.raw<Member | null>(null)
let demo = $state.raw<Demo | null>(null)

/** Who is signed in, and whether the installation still needs its first account. */
export const session = {
  get loaded() {
    return loaded
  },
  get setupRequired() {
    return setupRequired
  },
  /** Whether Turtle mails invitations and password reset links. */
  get sendsMail() {
    return sendsMail
  },
  get member() {
    return member
  },
  /** What a demo tells everyone; null when this Turtle is not a demo. */
  get demo() {
    return demo
  },
  /** Asks the server. Called once on start; the operations below keep it current afterwards. */
  async load() {
    const current = await call('get_session', {})
    setupRequired = current.setup_required
    sendsMail = current.sends_mail
    member = current.member ?? null
    demo = current.demo ?? null
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
