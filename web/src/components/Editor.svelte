<script lang="ts">
  import { Editor } from '@tiptap/core'
  import Collaboration from '@tiptap/extension-collaboration'
  import CollaborationCaret from '@tiptap/extension-collaboration-caret'
  import StarterKit from '@tiptap/starter-kit'
  import type { Awareness } from 'y-protocols/awareness'
  import type * as Y from 'yjs'
  import { AgentChanges } from '../lib/agent-changes'
  import { personColor, type Person } from '../lib/person'

  const {
    doc,
    connection,
    person,
    editable = true,
    onready,
  }: {
    doc: Y.Doc
    /** Where the cursors of the others come from. */
    connection: { awareness: Awareness }
    person: Person
    /** False for someone who can only read the Page. */
    editable?: boolean
    /** Called with the editor once it runs. It lives exactly as long as this component. */
    onready: (editor: Editor) => void
  } = $props()

  let element: HTMLDivElement

  $effect(() => {
    const editor = new Editor({
      element,
      extensions: [
        // Underline is off because Markdown cannot express it (ADR-0006).
        // The trailing node is off because every open editor would append its
        // own empty paragraph at the same moment, and they would pile up.
        StarterKit.configure({ undoRedo: false, underline: false, trailingNode: false }),
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
      autofocus: editable,
      editorProps: { attributes: { 'aria-label': 'Page content' } },
    })
    onready(editor)
    return () => editor.destroy()
  })
</script>

<div class="editor" class:readonly={!editable} bind:this={element}></div>

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
