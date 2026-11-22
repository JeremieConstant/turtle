/*
 * Images in a Page. An image stands in the text like a word, which is how
 * Markdown writes it (![description](address)), and points at an Attachment
 * of the Page. Pasted or dropped files are attached, and those Turtle shows
 * as images are put where they were pasted or dropped.
 */
import { Node, mergeAttributes } from '@tiptap/core'
import { Plugin, PluginKey } from '@tiptap/pm/state'
import type { EditorView } from '@tiptap/pm/view'
import {
  absolutePositionToRelativePosition,
  relativePositionToAbsolutePosition,
  ySyncPluginKey,
} from '@tiptap/y-tiptap'
import type { Attachment } from './operations.gen'

export type ImageOptions = {
  /** Attaches a file to the Page and returns it, or null if that failed (and was reported). */
  attach: (file: File) => Promise<Attachment | null>
}

export const Image = Node.create<ImageOptions>({
  name: 'image',
  inline: true,
  group: 'inline',
  atom: true,
  draggable: true,

  addOptions() {
    return { attach: async () => null }
  },

  addAttributes() {
    return {
      src: { default: '' },
      alt: { default: '' },
    }
  },

  parseHTML() {
    return [{ tag: 'img[src]' }]
  },

  renderHTML({ HTMLAttributes }) {
    return ['img', mergeAttributes(HTMLAttributes, { loading: 'lazy' })]
  },

  addProseMirrorPlugins() {
    const attach = this.options.attach
    const type = this.type

    /** Attaches the files, then puts each image in at `at`, one after the other. */
    async function attachAll(view: EditorView, files: File[], at: number) {
      // Others keep editing while the files are sent, so the place is kept
      // in the shared document, where it moves with their changes.
      const place = keep(view, at)
      for (const file of files) {
        const attached = await attach(file)
        if (!attached?.shown_as_image || view.isDestroyed) continue
        const image = type.create({ src: attached.url, alt: attached.name })
        const now = Math.min(place.now() ?? at, view.state.doc.content.size)
        view.dispatch(view.state.tr.insert(now, image))
      }
    }

    return [
      new Plugin({
        key: new PluginKey('imageFiles'),
        props: {
          handlePaste(view, event) {
            const files = [...(event.clipboardData?.files ?? [])]
            if (!view.editable || files.length === 0) return false
            event.preventDefault()
            void attachAll(view, files, view.state.selection.from)
            return true
          },
          handleDrop(view, event, _slice, moved) {
            const files = [...(event.dataTransfer?.files ?? [])]
            if (moved || !view.editable || files.length === 0) return false
            event.preventDefault()
            const at = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos ?? view.state.selection.from
            void attachAll(view, files, at)
            return true
          },
        },
      }),
    ]
  },
})

/**
 * A place in the content that moves with everyone's edits: `now()` says
 * where it is now, or null if it can no longer be found. It is tied to what
 * follows it, so each image put in there lands after the one before.
 */
function keep(view: EditorView, at: number): { now: () => number | null } {
  const sync = ySyncPluginKey.getState(view.state)
  if (!sync?.binding) return { now: () => at }
  const relative = absolutePositionToRelativePosition(at, sync.type, sync.binding.mapping)
  return {
    now: () => {
      const current = ySyncPluginKey.getState(view.state)
      if (!current?.binding) return null
      return relativePositionToAbsolutePosition(current.doc, current.type, relative, current.binding.mapping)
    },
  }
}
