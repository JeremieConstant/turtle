import * as Y from 'yjs'
import {
  Awareness,
  applyAwarenessUpdate,
  encodeAwarenessUpdate,
  removeAwarenessStates,
} from 'y-protocols/awareness'

// The first byte of every frame says what the rest is. Mirrors the server.
const FRAME_UPDATE = 0
const FRAME_PRESENCE = 1
const FRAME_PRESENCE_REQUEST = 2
const FRAME_AGENT_UPDATE = 3
const FRAME_TAGS = 4

export type Status = 'connecting' | 'connected' | 'disconnected'

/** An Agent as Turtle names it with its change: with the Member it acts for, unless it is a standalone Agent. */
export type ChangingAgent = { name: string; agent: true; member?: string }

/** Keeps a Yjs document and its presence in sync with the Turtle server. */
export class Connection {
  readonly awareness: Awareness
  private socket: WebSocket | null = null
  private hasLoaded = false
  private closed = false
  private retry: ReturnType<typeof setTimeout> | undefined
  private readonly leave = () => {
    removeAwarenessStates(this.awareness, [this.doc.clientID], 'unload')
  }

  constructor(
    private readonly url: string,
    private readonly doc: Y.Doc,
    private readonly onStatus: (status: Status) => void,
    /** Called once, when the content stored on the server has arrived. */
    private readonly onLoaded: () => void,
    /**
     * Called with an Agent's change, which arrives naming the Agent. `apply`
     * puts it into the document, at once; whatever the document changes
     * while it runs is that Agent's change and nothing else.
     */
    private readonly onAgentChange: (agent: ChangingAgent, apply: () => void) => void = (_agent, apply) => apply(),
    /** Called with the Page's Tags whenever someone changes them. */
    private readonly onTags: (tags: string[]) => void = () => {},
  ) {
    this.awareness = new Awareness(doc)

    doc.on('update', (update: Uint8Array, origin: unknown) => {
      if (origin !== this) this.send(FRAME_UPDATE, update)
    })
    this.awareness.on(
      'update',
      (change: { added: number[]; updated: number[]; removed: number[] }, origin: unknown) => {
        if (origin === this) return
        const clients = [...change.added, ...change.updated, ...change.removed]
        this.send(FRAME_PRESENCE, encodeAwarenessUpdate(this.awareness, clients))
      },
    )
    window.addEventListener('beforeunload', this.leave)

    this.open()
  }

  /** Leaves the Page for good: the others see this person go, and nothing reconnects. */
  close() {
    this.closed = true
    clearTimeout(this.retry)
    window.removeEventListener('beforeunload', this.leave)
    this.leave()
    this.socket?.close()
    this.awareness.destroy()
  }

  private open() {
    this.onStatus('connecting')
    const socket = new WebSocket(this.url)
    socket.binaryType = 'arraybuffer'
    this.socket = socket

    socket.onopen = () => {
      this.onStatus('connected')
      // After a reconnect, hand over whatever was typed while offline.
      if (this.hasLoaded) this.send(FRAME_UPDATE, Y.encodeStateAsUpdate(this.doc))
      this.announce()
    }
    socket.onmessage = event => {
      const data = new Uint8Array(event.data as ArrayBuffer)
      const payload = data.subarray(1)
      if (data[0] === FRAME_UPDATE) {
        this.applyUpdate(payload)
      } else if (data[0] === FRAME_AGENT_UPDATE) {
        const { agent, update } = readAgentChange(payload)
        // A change whose Agent cannot be read still counts; it only goes unnamed.
        if (agent) this.onAgentChange(agent, () => this.applyUpdate(update))
        else this.applyUpdate(update)
      } else if (data[0] === FRAME_PRESENCE) {
        applyAwarenessUpdate(this.awareness, payload, this)
      } else if (data[0] === FRAME_PRESENCE_REQUEST) {
        this.announce()
      } else if (data[0] === FRAME_TAGS) {
        const tags = readTags(payload)
        if (tags) this.onTags(tags)
      }
    }
    socket.onclose = () => {
      this.socket = null
      if (this.closed) return
      this.onStatus('disconnected')
      const others = [...this.awareness.getStates().keys()].filter(id => id !== this.doc.clientID)
      removeAwarenessStates(this.awareness, others, this)
      this.retry = setTimeout(() => this.open(), 1000)
    }
  }

  private applyUpdate(update: Uint8Array) {
    Y.applyUpdate(this.doc, update, this)
    if (!this.hasLoaded) {
      this.hasLoaded = true
      this.onLoaded()
    }
  }

  private announce() {
    if (this.awareness.getLocalState() === null) return
    this.send(FRAME_PRESENCE, encodeAwarenessUpdate(this.awareness, [this.doc.clientID]))
  }

  private send(kind: number, payload: Uint8Array) {
    if (this.socket?.readyState !== WebSocket.OPEN) return
    const frame = new Uint8Array(payload.length + 1)
    frame[0] = kind
    frame.set(payload, 1)
    this.socket.send(frame)
  }
}

/** The Tags in the payload of a Tags frame; none if they cannot be read. */
function readTags(payload: Uint8Array): string[] | null {
  try {
    const tags: unknown = JSON.parse(new TextDecoder().decode(payload))
    return Array.isArray(tags) && tags.every(tag => typeof tag === 'string') ? tags : null
  } catch {
    return null
  }
}

/** The Agent and the update in the payload of an Agent's change; no Agent if it cannot be read. */
function readAgentChange(payload: Uint8Array): { agent: ChangingAgent | null; update: Uint8Array } {
  // The Agent's length comes first, as an unsigned varint like Yjs writes them.
  let length = 0
  let at = 0
  for (let shift = 0; at < payload.length; shift += 7) {
    const byte = payload[at++]
    length += (byte & 0x7f) * 2 ** shift
    if (byte < 0x80) break
  }
  const update = payload.subarray(at + length)
  try {
    return { agent: JSON.parse(new TextDecoder().decode(payload.subarray(at, at + length))) as ChangingAgent, update }
  } catch {
    return { agent: null, update }
  }
}
