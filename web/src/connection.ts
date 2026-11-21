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

export type Status = 'connecting' | 'connected' | 'disconnected'

/** Keeps a Yjs document and its presence in sync with the Turtle server. */
export class Connection {
  readonly awareness: Awareness
  private socket: WebSocket | null = null
  private hasLoaded = false

  constructor(
    private readonly url: string,
    private readonly doc: Y.Doc,
    private readonly onStatus: (status: Status) => void,
    /** Called once, when the content stored on the server has arrived. */
    private readonly onLoaded: () => void,
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
    window.addEventListener('beforeunload', () => {
      removeAwarenessStates(this.awareness, [doc.clientID], 'unload')
    })

    this.open()
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
        Y.applyUpdate(this.doc, payload, this)
        if (!this.hasLoaded) {
          this.hasLoaded = true
          this.onLoaded()
        }
      } else if (data[0] === FRAME_PRESENCE) {
        applyAwarenessUpdate(this.awareness, payload, this)
      } else if (data[0] === FRAME_PRESENCE_REQUEST) {
        this.announce()
      }
    }
    socket.onclose = () => {
      this.socket = null
      this.onStatus('disconnected')
      const others = [...this.awareness.getStates().keys()].filter(id => id !== this.doc.clientID)
      removeAwarenessStates(this.awareness, others, this)
      setTimeout(() => this.open(), 1000)
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
