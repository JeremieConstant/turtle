import { mergeAttributes, Node } from '@tiptap/core'

/** The kinds of callout, as the editor stores them; in capitals each is its Markdown marker, such as `[!NOTE]`. */
export const CALLOUT_KINDS = ['note', 'tip', 'warning', 'danger'] as const
export type CalloutKind = (typeof CALLOUT_KINDS)[number]

export const CALLOUT_LABELS: Record<CalloutKind, string> = {
  note: 'Note',
  tip: 'Tip',
  warning: 'Warning',
  danger: 'Danger',
}

/** The kind a stored name stands for; one this version does not know is a note, as the server reads it. */
export function calloutKind(name: unknown): CalloutKind {
  return CALLOUT_KINDS.find(kind => kind === name) ?? 'note'
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    callout: {
      /**
       * Puts the selected blocks in a callout of the given kind. Inside one
       * of another kind it changes the kind; inside one of this kind it takes
       * the callout away.
       */
      toggleCallout: (kind: CalloutKind) => ReturnType
    }
  }
}

/** A highlighted passage of a given kind; to Markdown a quote whose first line names the kind. */
export const Callout = Node.create({
  name: 'callout',
  group: 'block',
  content: 'block+',
  defining: true,

  addAttributes() {
    return {
      kind: {
        default: 'note',
        parseHTML: element => calloutKind(element.getAttribute('data-callout')),
        renderHTML: attributes => ({ 'data-callout': calloutKind(attributes.kind) }),
      },
    }
  },

  parseHTML() {
    return [{ tag: 'div[data-callout]' }]
  },

  renderHTML({ node, HTMLAttributes }) {
    const label = CALLOUT_LABELS[calloutKind(node.attrs.kind)]
    return ['div', mergeAttributes(HTMLAttributes, { class: 'callout', role: 'note', 'aria-label': label }), 0]
  },

  addCommands() {
    return {
      toggleCallout:
        kind =>
        ({ editor, commands }) => {
          if (editor.isActive(this.name, { kind })) return commands.lift(this.name)
          if (editor.isActive(this.name)) return commands.updateAttributes(this.name, { kind })
          return commands.wrapIn(this.name, { kind })
        },
    }
  },
})
