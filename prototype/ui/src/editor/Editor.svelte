<script lang="ts">
  /*
   * The Page editor. It offers only what Markdown can express (ADR-0006):
   * headings, paragraphs, lists, task lists, simple tables, code blocks,
   * quotes, callouts, links to other Pages and images. Nothing shows Markdown;
   * typing "/" in an empty line opens the list of blocks.
   */
  import { Editor } from '@tiptap/core'
  import Image from '@tiptap/extension-image'
  import { TaskItem, TaskList } from '@tiptap/extension-list'
  import { TableKit } from '@tiptap/extension-table'
  import { Placeholder } from '@tiptap/extensions'
  import StarterKit from '@tiptap/starter-kit'
  import { untrack } from 'svelte'
  import type { Author } from '../lib/mock/types'
  import { router } from '../lib/router.svelte'
  import { AgentChanges, Callout, RemoteCarets } from './extensions'
  import PickerDialog from './PickerDialog.svelte'

  let {
    html,
    editable,
    onready,
    onchange,
  }: {
    html: string
    editable: boolean
    onready?: (editor: Editor) => void
    onchange?: (editor: Editor, by: Author | null) => void
  } = $props()

  let element: HTMLDivElement
  let editor = $state<Editor | null>(null)

  /* ---- The "/" menu --------------------------------------------------- */

  type Block = { label: string; keys: string; run: (e: Editor) => void }
  const BLOCKS: Block[] = [
    { label: 'Heading', keys: 'h2 heading title', run: (e) => e.chain().focus().setHeading({ level: 2 }).run() },
    { label: 'Subheading', keys: 'h3 subheading', run: (e) => e.chain().focus().setHeading({ level: 3 }).run() },
    { label: 'Bulleted list', keys: 'bullet list ul', run: (e) => e.chain().focus().toggleBulletList().run() },
    { label: 'Numbered list', keys: 'numbered ordered list ol', run: (e) => e.chain().focus().toggleOrderedList().run() },
    { label: 'Task list', keys: 'task todo checkbox', run: (e) => e.chain().focus().toggleTaskList().run() },
    {
      label: 'Table',
      keys: 'table grid',
      run: (e) => e.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run(),
    },
    { label: 'Code block', keys: 'code pre snippet', run: (e) => e.chain().focus().toggleCodeBlock().run() },
    { label: 'Quote', keys: 'quote blockquote', run: (e) => e.chain().focus().toggleBlockquote().run() },
    { label: 'Callout', keys: 'callout note info', run: (e) => e.chain().focus().wrapIn('callout').run() },
    { label: 'Link to a Page', keys: 'link page', run: () => picker.open('page') },
    { label: 'Image', keys: 'image picture', run: () => picker.open('image') },
  ]

  let slash = $state<null | { query: string; from: number; to: number; x: number; y: number }>(null)
  let active = $state(0)
  let picker: PickerDialog
  const matches = $derived(
    slash ? BLOCKS.filter((b) => (b.label + ' ' + b.keys).toLowerCase().includes(slash!.query.toLowerCase())) : [],
  )

  function readSlash(e: Editor) {
    const { selection } = e.state
    const head = selection.$from
    const parent = head.parent
    if (!selection.empty || !parent.isTextblock || parent.type.name !== 'paragraph') return (slash = null)
    const text = parent.textContent
    const match = /^\/([\w ]{0,20})$/.exec(text)
    if (!match || head.parentOffset !== text.length) return (slash = null)
    const start = head.start()
    const coords = e.view.coordsAtPos(start)
    const box = element.getBoundingClientRect()
    if (!slash) active = 0
    slash = { query: match[1], from: start, to: start + text.length, x: coords.left - box.left, y: coords.bottom - box.top }
  }

  function choose(block: Block) {
    if (!editor || !slash) return
    editor.chain().focus().deleteRange({ from: slash.from, to: slash.to }).run()
    slash = null
    block.run(editor)
  }

  $effect(() => {
    // Built once; later changes to the props reach the editor through its own API.
    const e = untrack(() => new Editor({
      element,
      extensions: [
        // Underline is off: Markdown cannot express it (ADR-0006).
        StarterKit.configure({
          underline: false,
          heading: { levels: [1, 2, 3] },
          link: { openOnClick: false, autolink: true, HTMLAttributes: { target: null, rel: null } },
        }),
        TaskList,
        TaskItem.configure({ nested: true }),
        TableKit.configure({ table: { resizable: false } }),
        Image,
        Callout,
        Placeholder.configure({ placeholder: 'Type / for headings, lists, tables and more' }),
        RemoteCarets,
        AgentChanges,
      ],
      content: html,
      editable,
      editorProps: {
        attributes: { 'aria-label': 'Page content' },
        handleKeyDown(_view, event) {
          if (!slash || !matches.length) return false
          if (event.key === 'ArrowDown') active = (active + 1) % matches.length
          else if (event.key === 'ArrowUp') active = (active - 1 + matches.length) % matches.length
          else if (event.key === 'Enter') choose(matches[active])
          else if (event.key === 'Escape') slash = null
          else return false
          return true
        },
        handleClick(_view, _pos, event) {
          // Links to other Pages open inside the app.
          const link = (event.target as Element).closest<HTMLAnchorElement>('a[href^="/p/"]')
          if (!link) return false
          router.follow(event, link.getAttribute('href')!)
          return true
        },
      },
      onTransaction: ({ editor: ed, transaction }) => {
        if (transaction.docChanged) onchange?.(ed, (transaction.getMeta('remote') as Author | undefined) ?? null)
        // Only typing and caret moves can open the menu. A blur also arrives as a
        // transaction, sometimes while Svelte is removing the editor.
        if (editable && (transaction.docChanged || transaction.selectionSet)) readSlash(ed)
      },
      onBlur: () => setTimeout(() => (slash = null), 120),
    }))
    editor = e
    onready?.(e)
    return () => e.destroy()
  })

  $effect(() => {
    editor?.setEditable(editable)
  })
</script>

<div class="editor" class:readonly={!editable} bind:this={element}>
  {#if slash && matches.length}
    <div class="slash" role="listbox" aria-label="Insert a block" style:left="{slash.x}px" style:top="{slash.y + 6}px">
      {#each matches as block, i (block.label)}
        <button
          role="option"
          aria-selected={i === active}
          class:active={i === active}
          onmousedown={(e) => (e.preventDefault(), choose(block))}
          onmouseenter={() => (active = i)}
        >
          {block.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<PickerDialog bind:this={picker} {editor} />

<style>
  .editor {
    position: relative;
  }

  .editor :global(.tiptap) {
    min-height: 40vh;
    outline: none;
    font-size: var(--text-base);
    line-height: var(--leading-body);
    color: var(--color-text);
    padding-bottom: var(--space-16);
  }

  .editor :global(.tiptap > *) {
    margin-top: 0;
    margin-bottom: 0.85em;
  }

  .editor :global(.tiptap h1),
  .editor :global(.tiptap h2),
  .editor :global(.tiptap h3) {
    line-height: var(--leading-tight);
    font-weight: var(--weight-strong);
  }

  .editor :global(.tiptap h2) {
    margin-top: 1.9em;
    margin-bottom: 0.6em;
    font-size: var(--text-h2);
    letter-spacing: var(--tracking-heading);
  }

  .editor :global(.tiptap h3) {
    margin-top: 1.6em;
    margin-bottom: 0.5em;
    font-size: var(--text-h3);
    letter-spacing: var(--tracking-heading);
  }

  .editor :global(.tiptap a) {
    color: var(--color-text);
    text-decoration: underline;
    text-decoration-color: var(--color-faint);
    text-underline-offset: 3px;
    text-decoration-thickness: 1px;
    cursor: pointer;
    transition: text-decoration-color var(--dur-fast) var(--ease-out);
  }

  .editor :global(.tiptap a:hover) {
    text-decoration-color: var(--color-text);
  }

  .editor :global(.tiptap code) {
    padding: 0.1em 0.3em;
    border-radius: var(--radius-s);
    background: var(--color-sunken);
    font-family: var(--font-mono);
    font-size: 0.86em;
  }

  .editor :global(.tiptap pre) {
    padding: var(--space-4);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
    background: var(--color-side);
    overflow-x: auto;
    line-height: var(--leading-code);
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

  .editor :global(.tiptap [data-callout]) {
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-m);
    background: var(--color-sunken);
  }

  .editor :global(.tiptap [data-callout] p) {
    margin: 0 0 0.6em;
  }

  .editor :global(.tiptap [data-callout] p:last-child) {
    margin-bottom: 0;
  }

  .editor :global(.tiptap ul),
  .editor :global(.tiptap ol) {
    padding-left: 1.4em;
  }

  .editor :global(.tiptap li p) {
    margin: 0.2em 0;
  }

  .editor :global(.tiptap ul[data-type='taskList']) {
    padding-left: 0.1em;
    list-style: none;
  }

  .editor :global(.tiptap ul[data-type='taskList'] li) {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
  }

  .editor :global(.tiptap ul[data-type='taskList'] li > label) {
    flex: none;
    margin-top: 0.42em;
  }

  .editor :global(.tiptap ul[data-type='taskList'] li > div) {
    flex: 1;
  }

  .editor :global(.tiptap ul[data-type='taskList'] input) {
    width: 15px;
    height: 15px;
    margin: 0;
    accent-color: var(--color-text);
  }

  .editor :global(.tiptap li[data-checked='true'] > div) {
    color: var(--color-muted);
    text-decoration: line-through;
    text-decoration-color: var(--color-faint);
  }

  .editor :global(.tiptap .tableWrapper) {
    overflow-x: auto;
  }

  .editor :global(.tiptap table) {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-m);
  }

  .editor :global(.tiptap th),
  .editor :global(.tiptap td) {
    padding: var(--space-2) var(--space-3);
    border-bottom: var(--line) solid var(--color-line);
    text-align: left;
    vertical-align: top;
  }

  .editor :global(.tiptap th) {
    border-bottom-color: var(--color-line-strong);
    font-weight: var(--weight-strong);
  }

  .editor :global(.tiptap th p),
  .editor :global(.tiptap td p) {
    margin: 0;
  }

  .editor :global(.tiptap .selectedCell) {
    background: var(--color-focus-soft);
  }

  .editor :global(.tiptap img) {
    max-width: 100%;
    border-radius: var(--radius-m);
  }

  .editor :global(.tiptap img.ProseMirror-selectednode) {
    outline: var(--line-thick) solid var(--color-focus);
  }

  .editor :global(.tiptap p.is-editor-empty:first-child::before),
  .editor :global(.tiptap p.is-empty:only-child::before) {
    content: attr(data-placeholder);
    float: left;
    height: 0;
    color: var(--color-faint);
    pointer-events: none;
  }

  .readonly :global(.tiptap p.is-empty::before) {
    content: none;
  }

  /* Other people's cursors. They move with the text, never animated. */
  .editor :global(.remote-caret) {
    position: relative;
    margin: 0 -1px;
    border-left: 2px solid var(--person);
    pointer-events: none;
  }

  .editor :global(.remote-caret-label) {
    position: absolute;
    bottom: 100%;
    left: -2px;
    padding: 1px 5px;
    border-radius: 3px 3px 3px 0;
    background: var(--person);
    color: var(--color-on-color);
    font-size: 11px;
    font-weight: var(--weight-medium);
    line-height: 1.4;
    white-space: nowrap;
    user-select: none;
  }

  .editor :global(.remote-selection) {
    background: color-mix(in srgb, var(--person) 16%, transparent);
  }

  /* An Agent's change: a tinted band with a left rule and the Agent's name
     in the margin. It holds for a moment, then fades to nothing. */
  .editor :global(.agent-change) {
    position: relative;
    border-radius: var(--radius-s);
    box-shadow: -10px 0 0 var(--color-agent-soft), 10px 0 0 var(--color-agent-soft);
    background: var(--color-agent-soft);
    animation:
      agent-arrive var(--dur-slow) var(--ease-out),
      agent-fade var(--dur-agent) var(--ease-in-out) 5s forwards;
  }

  .editor :global(.agent-change::before) {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: -12px;
    width: 2px;
    border-radius: 2px;
    background: var(--color-agent);
    animation: agent-gone var(--dur-agent) var(--ease-in-out) 5s forwards;
  }

  .editor :global(.agent-label) {
    position: absolute;
    top: 0.15em;
    right: calc(100% + 22px);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 6px;
    border: var(--line) solid var(--color-agent);
    border-radius: var(--radius-s);
    background: var(--color-bg);
    color: var(--color-agent);
    font-size: 11px;
    font-weight: var(--weight-medium);
    line-height: 18px;
    white-space: nowrap;
    user-select: none;
    animation:
      agent-label-in var(--dur-slow) var(--ease-out),
      agent-gone var(--dur-agent) var(--ease-in-out) 5s forwards;
  }

  .editor :global(.agent-label::before) {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 1px;
    background: var(--color-agent);
    transform: rotate(45deg);
  }

  @keyframes agent-arrive {
    from {
      background: transparent;
      box-shadow: -10px 0 0 transparent, 10px 0 0 transparent;
      clip-path: inset(0 100% 0 0);
    }
  }

  @keyframes agent-fade {
    to {
      background: transparent;
      box-shadow: -10px 0 0 transparent, 10px 0 0 transparent;
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
      transform: translateX(4px);
    }
  }

  @media (max-width: 1100px) {
    .editor :global(.agent-label) {
      top: -1.6em;
      right: auto;
      left: 0;
    }
  }

  .slash {
    position: absolute;
    z-index: var(--layer-menu);
    display: grid;
    min-width: 200px;
    padding: var(--space-1);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
    background: var(--color-surface);
    box-shadow: var(--shadow-float);
    animation: menu-in var(--dur-fast) var(--ease-out);
    transform-origin: top left;
  }

  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-2px) scale(0.98);
    }
  }

  .slash button {
    height: 30px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    font-size: var(--text-m);
    text-align: left;
  }

  .slash button.active {
    background: var(--color-hover);
  }
</style>
