import { Node } from '@tiptap/core'

/** The colours a Status comes in, by the names its Markdown and the server use; grey first, as the default. */
export const STATUS_COLORS = ['grey', 'blue', 'green', 'yellow', 'red', 'purple'] as const
export type StatusColor = (typeof STATUS_COLORS)[number]

/** A colour by its name; any other is grey, as the server reads it. */
export function statusColor(name: unknown): StatusColor {
  return STATUS_COLORS.find(color => color === name) ?? 'grey'
}

/**
 * A Status's text as it can be kept: as typed, but on one line and with a
 * `/` for a `|`, since a directive in running text holds neither (ADR-0013),
 * as the server normalises it.
 */
export function statusText(typed: string): string {
  return typed.replace(/\r\n|[\r\n]/g, ' ').replace(/\|/g, '/')
}

/** What a newly inserted Status says until it is given its own text. */
export const NEW_STATUS = 'Status'

/** The event a Status sends when it is clicked to be changed, with where it stands in the document. */
export const EDIT_STATUS = 'turtle-edit-status'
export type EditStatus = CustomEvent<{ pos: number; element: HTMLElement }>

/**
 * A Status: a short coloured marker in running text, such as ON TRACK in
 * green, written `:status[ON TRACK]{color=green}`. Its text is kept as typed
 * and shown in uppercase, and it is one piece: clicking it asks to change
 * its text or colour (see `EDIT_STATUS`).
 */
export const Status = Node.create({
  name: 'status',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  marks: '',

  addAttributes() {
    return {
      text: {
        default: '',
        parseHTML: element => element.textContent ?? '',
        // Shown as the element's text, not kept as an attribute in the page.
        renderHTML: () => ({}),
      },
      color: {
        default: 'grey',
        parseHTML: element => statusColor(element.getAttribute('data-color')),
        renderHTML: attributes => ({ 'data-color': statusColor(attributes.color) }),
      },
    }
  },

  parseHTML() {
    return [{ tag: 'span[data-status]' }]
  },

  renderHTML({ node, HTMLAttributes }) {
    return ['span', { ...HTMLAttributes, 'data-status': '', class: `status status-${statusColor(node.attrs.color)}` }, String(node.attrs.text)]
  },

  addNodeView() {
    return ({ node, getPos, editor }) => {
      const element = document.createElement('span')
      element.dataset.status = ''
      const show = (shown: typeof node) => {
        element.className = `status status-${statusColor(shown.attrs.color)}`
        element.textContent = String(shown.attrs.text)
      }
      show(node)
      element.addEventListener('click', () => {
        const pos = getPos()
        if (!editor.isEditable || typeof pos !== 'number') return
        element.dispatchEvent(new CustomEvent(EDIT_STATUS, { bubbles: true, detail: { pos, element } }))
      })
      return {
        dom: element,
        update: updated => {
          if (updated.type !== node.type) return false
          show(updated)
          return true
        },
      }
    }
  },
})
