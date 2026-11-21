<script lang="ts">
  import { Editor } from '@tiptap/core'
  import Collaboration from '@tiptap/extension-collaboration'
  import CollaborationCaret from '@tiptap/extension-collaboration-caret'
  import StarterKit from '@tiptap/starter-kit'
  import type { Awareness } from 'y-protocols/awareness'
  import type * as Y from 'yjs'
  import type { Person } from '../lib/person'

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
        CollaborationCaret.configure({ provider: connection, user: person }),
      ],
      editable,
      autofocus: editable,
    })
    onready(editor)
    return () => editor.destroy()
  })
</script>

<div class="editor" bind:this={element}></div>

<style>
  /* TipTap builds the content itself, so its elements are reached from the
     component's own element instead of carrying the component's scope. */
  .editor :global(.tiptap) {
    min-height: var(--height-editor);
    outline: none;
  }

  .editor :global(.tiptap > :first-child) {
    margin-top: 0;
  }

  .editor :global(.tiptap :is(h1, h2, h3)) {
    line-height: var(--leading-tight);
  }

  .editor :global(.tiptap blockquote) {
    margin-left: 0;
    padding-left: var(--space-4);
    border-left: var(--line-width-quote) solid var(--color-line);
    color: var(--color-muted);
  }

  .editor :global(.tiptap code) {
    padding: var(--space-hair) var(--space-1);
    border-radius: var(--radius-s);
    background: var(--color-line);
    font-family: var(--font-mono);
    font-size: var(--text-code-inline);
  }

  .editor :global(.tiptap pre) {
    padding: var(--space-3);
    border-radius: var(--radius-l);
    background: var(--color-line);
    overflow-x: auto;
  }

  .editor :global(.tiptap pre code) {
    padding: 0;
    background: none;
  }

  .editor :global(.tiptap a) {
    color: var(--color-accent);
  }

  .editor :global(.tiptap hr) {
    border: none;
    border-top: var(--line-width) solid var(--color-line);
  }

  /* Other people's cursors */
  .editor :global(.collaboration-carets__caret) {
    position: relative;
    margin-left: calc(-1 * var(--line-width));
    margin-right: calc(-1 * var(--line-width));
    border-left: var(--line-width) solid;
    border-right: var(--line-width) solid;
    pointer-events: none;
    word-break: normal;
  }

  .editor :global(.collaboration-carets__label) {
    position: absolute;
    top: var(--offset-caret-label);
    left: calc(-1 * var(--line-width));
    padding: 0 var(--space-2);
    border-radius: var(--radius-s) var(--radius-s) var(--radius-s) 0;
    color: var(--color-on-person);
    font-size: var(--text-xs);
    font-weight: var(--weight-strong);
    line-height: var(--leading-code);
    white-space: nowrap;
    user-select: none;
  }
</style>
