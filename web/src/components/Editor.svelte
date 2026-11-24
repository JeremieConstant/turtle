<script lang="ts">
  import { Editor } from '@tiptap/core'
  import Collaboration from '@tiptap/extension-collaboration'
  import CollaborationCaret from '@tiptap/extension-collaboration-caret'
  import type { Awareness } from 'y-protocols/awareness'
  import type * as Y from 'yjs'
  import { AgentChanges } from '../lib/agent-changes'
  import { contentExtensions } from '../lib/extensions'
  import type { ImageOptions } from '../lib/image'
  import { personColor, type Person } from '../lib/person'

  const {
    doc,
    connection,
    person,
    editable = true,
    autofocus = editable,
    label = 'Page content',
    compact = false,
    attach,
    pageId = null,
    onready,
  }: {
    doc: Y.Doc
    /** Where the cursors of the others come from. */
    connection: { awareness: Awareness }
    person: Person
    /** False for someone who can only read the Page. */
    editable?: boolean
    /** Whether the cursor goes into the content as soon as it shows. */
    autofocus?: boolean
    /** What the content is called for screen readers. */
    label?: string
    /** Fits a short text in a box rather than a whole Page. */
    compact?: boolean
    /** Attaches a pasted or dropped file to the Page. */
    attach: ImageOptions['attach']
    /** The Page the content is in; absent for content in no Page yet. */
    pageId?: string | null
    /** Called with the editor once it runs. It lives exactly as long as this component. */
    onready: (editor: Editor) => void
  } = $props()

  let element: HTMLDivElement

  $effect(() => {
    const editor = new Editor({
      element,
      extensions: [
        ...contentExtensions({ attach }, pageId),
        Collaboration.configure({ document: doc }),
        CollaborationCaret.configure({
          provider: connection,
          user: person,
          // Someone's cursor carries their name, and their selection is tinted in their
          // colour and, on hover, names them. The colour is looked up from the number
          // they sent, so it is the one of this browser's appearance.
          render: user => {
            const caret = document.createElement('span')
            caret.classList.add('collaboration-carets__caret')
            caret.style.setProperty('--person', personColor(user))
            const label = document.createElement('div')
            label.classList.add('collaboration-carets__label')
            label.textContent = String(user.name ?? '')
            caret.append(label)
            return caret
          },
          selectionRender: user => ({
            nodeName: 'span',
            class: 'remote-selection',
            style: `--person: ${personColor(user)}`,
            title: String(user.name ?? ''),
          }),
        }),
        AgentChanges,
      ],
      editable,
      autofocus,
      editorProps: { attributes: { 'aria-label': label } },
    })
    onready(editor)
    return () => editor.destroy()
  })
</script>

<div class="editor" class:readonly={!editable} class:compact bind:this={element}></div>

<style>
  .editor {
    position: relative;
  }

  /* TipTap builds the content itself, so its elements are reached from the
     component's own element instead of carrying the component's scope. */
  .editor :global(.tiptap) {
    min-height: var(--height-editor);
    padding-bottom: var(--space-16);
    outline: none;
    color: var(--color-text);
    font-size: var(--text-base);
    line-height: var(--leading-body);
  }

  .compact :global(.tiptap) {
    min-height: var(--height-quick-note);
    padding-bottom: 0;
  }

  .editor :global(.tiptap > *) {
    margin-top: 0;
    margin-bottom: var(--space-block);
  }

  .editor :global(.tiptap :is(h1, h2, h3)) {
    margin-top: var(--space-heading-above);
    margin-bottom: var(--space-heading-below);
    font-weight: var(--weight-strong);
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-heading);
  }

  .editor :global(.tiptap h1) {
    font-size: var(--text-h1);
    letter-spacing: var(--tracking-display);
  }

  .editor :global(.tiptap h2) {
    font-size: var(--text-h2);
  }

  .editor :global(.tiptap h3) {
    font-size: var(--text-h3);
  }

  .editor :global(.tiptap > :first-child) {
    margin-top: 0;
  }

  .editor :global(.tiptap a) {
    color: var(--color-text);
    text-decoration: underline;
    text-decoration-color: var(--color-faint);
    text-decoration-thickness: var(--line);
    text-underline-offset: var(--space-1);
    transition: text-decoration-color var(--dur-fast) var(--ease-out);
  }

  .editor :global(.tiptap a:hover) {
    text-decoration-color: var(--color-text);
  }

  /* A Page link reads as a link, with a small page in front of its title. */
  /* A Mention shows the person's mark and name. */
  .editor :global(.tiptap .mention) {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    /* As tall as the mark plus the same inset all round, so the mark sits centred and the pill stays within the line. */
    padding: var(--space-half) var(--space-2) var(--space-half) var(--space-half);
    line-height: var(--size-mention-picture);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-round);
    background: var(--color-surface);
    color: var(--color-text);
    font-weight: var(--weight-medium);
    text-decoration: none;
    vertical-align: baseline;
    white-space: nowrap;
    cursor: pointer;
  }

  .editor :global(.tiptap .mention.name),
  .editor :global(.tiptap .mention.unknown),
  .editor :global(.tiptap .mention.failed),
  .editor :global(.tiptap .mention.loading) {
    padding-left: var(--space-2);
    cursor: default;
  }

  .editor :global(.tiptap .mention:is(.unknown, .failed, .loading, .deactivated)) {
    color: var(--color-muted);
  }

  /* A flex box, not a line of text, so the mark is exactly its own height. */
  .editor :global(.tiptap .mention-mark) {
    display: inline-flex;
  }

  .editor :global(.tiptap .mention-mark:empty) {
    display: none;
  }

  .editor :global(.tiptap .mention.ProseMirror-selectednode) {
    background: var(--color-selected);
  }

  .editor :global(.tiptap .page-link) {
    cursor: pointer;
  }

  .editor :global(.tiptap .page-link::before) {
    content: '';
    display: inline-block;
    width: var(--size-page-link-icon);
    height: var(--size-page-link-icon);
    margin-right: var(--space-hair);
    vertical-align: var(--offset-page-link-icon);
    background: var(--color-muted);
    mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16' fill='none' stroke='black' stroke-width='1.5' stroke-linejoin='round'%3E%3Cpath d='M4 2.5h5l3 3v8H4zM9 2.5v3h3'/%3E%3C/svg%3E") center / contain no-repeat;
  }

  .editor :global(.tiptap .page-link.loading) {
    color: var(--color-muted);
  }

  /* One the reader may not follow: no title, no underline, nothing to click. */
  .editor :global(.tiptap .page-link.unavailable) {
    color: var(--color-muted);
    font-style: italic;
    text-decoration: none;
    cursor: default;
  }

  .editor :global(.tiptap .page-link.ProseMirror-selectednode) {
    border-radius: var(--radius-s);
    background: var(--color-selected);
  }

  /* A directive Turtle does not know: a locked grey placeholder naming it. */
  .editor :global(.tiptap .unknown-directive) {
    padding: var(--space-hair) var(--space-1);
    border-radius: var(--radius-s);
    background: var(--color-sunken);
    color: var(--color-muted);
    font-size: var(--text-code-inline);
    cursor: default;
    user-select: none;
  }

  .editor :global(.tiptap div.unknown-directive) {
    padding: var(--space-2) var(--space-3);
  }

  .editor :global(.tiptap .unknown-directive.ProseMirror-selectednode) {
    background: var(--color-selected);
  }

  /* A Status is changed with a click, while it can be edited. */
  .editor:not(.readonly) :global(.tiptap .status) {
    cursor: pointer;
  }

  .editor :global(.tiptap .status.ProseMirror-selectednode) {
    outline: var(--line-thick) solid var(--color-focus);
    outline-offset: var(--line);
  }

  .editor :global(.tiptap code) {
    padding: var(--space-hair) var(--space-1);
    border-radius: var(--radius-s);
    background: var(--color-sunken);
    font-family: var(--font-mono);
    font-size: var(--text-code-inline);
  }

  .editor :global(.tiptap pre) {
    padding: var(--space-4);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
    background: var(--color-side);
    line-height: var(--leading-code);
    overflow-x: auto;
  }

  .editor :global(.tiptap pre code) {
    padding: 0;
    background: none;
    font-size: var(--text-s);
  }

  .editor :global(.tiptap blockquote) {
    margin-left: 0;
    padding-left: var(--space-4);
    border-left: var(--line-quote) solid var(--color-line-strong);
    color: var(--color-text-2);
  }

  .editor :global(.tiptap :is(ul, ol)) {
    padding-left: var(--indent-list);
  }

  .editor :global(.tiptap li p) {
    margin: var(--space-list-item) 0;
  }

  /* A task list shows checkboxes instead of bullets. */
  .editor :global(.tiptap ul[data-type='taskList']) {
    padding-left: 0;
    list-style: none;
  }

  .editor :global(.tiptap li[data-checked]) {
    display: flex;
    align-items: baseline;
    gap: var(--gap-task);
  }

  .editor :global(.tiptap li[data-checked] > label) {
    flex: none;
    user-select: none;
  }

  .editor :global(.tiptap li[data-checked] > label input) {
    margin: 0;
    accent-color: var(--color-text);
    cursor: pointer;
  }

  .readonly :global(.tiptap li[data-checked] > label input) {
    cursor: default;
  }

  .editor :global(.tiptap li[data-checked] > div) {
    flex: 1;
    min-width: 0;
  }

  /* What is done steps back, but stays readable. */
  .editor :global(.tiptap li[data-checked='true'] > div > p) {
    color: var(--color-muted);
  }

  /* A callout is tinted in the colour of its kind and names it above its content. */
  .editor :global(.tiptap .callout) {
    --callout: var(--color-callout-note);
    --callout-soft: var(--color-callout-note-soft);
    padding: var(--space-3) var(--space-4);
    border-left: var(--line-quote) solid var(--callout);
    border-radius: var(--radius-m);
    background: var(--callout-soft);
  }

  .editor :global(.tiptap .callout[data-callout='tip']) {
    --callout: var(--color-callout-tip);
    --callout-soft: var(--color-callout-tip-soft);
  }

  .editor :global(.tiptap .callout[data-callout='warning']) {
    --callout: var(--color-callout-warning);
    --callout-soft: var(--color-callout-warning-soft);
  }

  .editor :global(.tiptap .callout[data-callout='danger']) {
    --callout: var(--color-callout-danger);
    --callout-soft: var(--color-callout-danger-soft);
  }

  .editor :global(.tiptap .callout::before) {
    content: attr(aria-label);
    display: block;
    margin-bottom: var(--space-1);
    color: var(--callout);
    font-size: var(--text-s);
    font-weight: var(--weight-strong);
    user-select: none;
  }

  .editor :global(.tiptap .callout > *) {
    margin-top: 0;
    margin-bottom: var(--space-block);
  }

  .editor :global(.tiptap .callout > :last-child) {
    margin-bottom: 0;
  }

  /* A table keeps to the width of the text and scrolls sideways when it is wider. */
  .editor :global(.tiptap .tableWrapper) {
    overflow-x: auto;
  }

  .editor :global(.tiptap table) {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  .editor :global(.tiptap :is(td, th)) {
    position: relative;
    padding: var(--space-1) var(--space-2);
    border: var(--line) solid var(--color-line);
    vertical-align: top;
    text-align: left;
  }

  .editor :global(.tiptap th) {
    background: var(--color-side);
    font-weight: var(--weight-strong);
  }

  .editor :global(.tiptap :is(td, th) p) {
    margin: 0;
  }

  /* Cells chosen together, to delete their rows or columns. */
  .editor :global(.tiptap .selectedCell) {
    background: var(--color-selected);
  }

  /* An image keeps to the width of the text and never stretches beyond its own. */
  .editor :global(.tiptap img) {
    max-width: 100%;
    height: auto;
    border-radius: var(--radius-s);
    vertical-align: bottom;
  }

  .editor :global(.tiptap img.ProseMirror-selectednode) {
    outline: var(--line-thick) solid var(--color-focus);
  }

  .editor :global(.tiptap hr) {
    border: none;
    border-top: var(--line) solid var(--color-line);
  }

  .readonly :global(.tiptap) {
    cursor: default;
  }

  /* Other people's cursors carry their name. They move with the text, never animated. */
  .editor :global(.collaboration-carets__caret) {
    position: relative;
    margin: 0 calc(-1 * var(--line));
    border-left: var(--line-thick) solid var(--person);
    pointer-events: none;
    word-break: normal;
  }

  .editor :global(.collaboration-carets__label) {
    position: absolute;
    bottom: 100%;
    left: calc(-1 * var(--line-thick));
    padding: var(--space-hair) var(--space-1);
    border-radius: var(--radius-s) var(--radius-s) var(--radius-s) 0;
    background: var(--person);
    color: var(--color-on-color);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-ui);
    white-space: nowrap;
    user-select: none;
  }

  .editor :global(.remote-selection) {
    background: color-mix(in srgb, var(--person) var(--mix-selection), transparent);
  }

  /* An Agent's change: a tinted band with a rule on the left and the Agent's
     name in the margin. It holds for a moment, then fades to nothing. */
  .editor :global(.agent-change) {
    position: relative;
    border-radius: var(--radius-s);
    background: var(--color-agent-soft);
    box-shadow:
      calc(-1 * var(--reach-agent-band)) 0 0 var(--color-agent-soft),
      var(--reach-agent-band) 0 0 var(--color-agent-soft);
    animation:
      agent-arrive var(--dur-slow) var(--ease-out),
      agent-fade var(--dur-agent) var(--ease-in-out) var(--delay-agent) forwards;
  }

  .editor :global(.agent-change::before) {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: calc(-1 * var(--offset-agent-rule));
    width: var(--line-thick);
    border-radius: var(--line-thick);
    background: var(--color-agent);
    animation: agent-gone var(--dur-agent) var(--ease-in-out) var(--delay-agent) forwards;
  }

  .editor :global(.agent-label) {
    position: absolute;
    top: var(--offset-agent-label-top);
    right: calc(100% + var(--offset-agent-label));
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-1);
    border: var(--line) solid var(--color-agent);
    border-radius: var(--radius-s);
    background: var(--color-bg);
    color: var(--color-agent);
    font-size: var(--text-agent-label);
    font-weight: var(--weight-medium);
    line-height: var(--leading-code);
    white-space: nowrap;
    user-select: none;
    animation:
      agent-label-in var(--dur-slow) var(--ease-out),
      agent-gone var(--dur-agent) var(--ease-in-out) var(--delay-agent) forwards;
  }

  /* The Agent mark: a small diamond, the same as beside its name above. */
  .editor :global(.agent-label::before) {
    content: '';
    width: var(--size-agent-diamond);
    height: var(--size-agent-diamond);
    border-radius: var(--space-hair);
    background: var(--color-agent);
    transform: rotate(45deg);
  }

  @keyframes agent-arrive {
    from {
      background: transparent;
      box-shadow: none;
      clip-path: inset(0 100% 0 0);
    }
  }

  @keyframes agent-fade {
    to {
      background: transparent;
      box-shadow: none;
    }
  }

  @keyframes agent-gone {
    to {
      opacity: 0;
    }
  }

  @keyframes agent-label-in {
    from {
      opacity: 0;
      transform: translateX(var(--space-1));
    }
  }

  /* Where the margin is too narrow for the name, it sits above the change. */
  @media (max-width: 1100px) {
    .editor :global(.agent-label) {
      top: var(--offset-caret-label);
      right: auto;
      left: 0;
    }
  }
</style>
