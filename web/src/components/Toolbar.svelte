<script lang="ts">
  import type { Editor } from '@tiptap/core'
  import type { Snippet } from 'svelte'

  const {
    editor,
    children,
  }: {
    /** `null` until the editor has started; the toolbar is empty until then. */
    editor: Editor | null
    /** Shown at the far end of the toolbar. */
    children?: Snippet
  } = $props()

  type Button = { label: string; title: string; run: (editor: Editor) => void; active?: (editor: Editor) => boolean }

  const buttons: Button[] = [
    { label: 'B', title: 'Bold', run: e => e.chain().focus().toggleBold().run(), active: e => e.isActive('bold') },
    { label: 'I', title: 'Italic', run: e => e.chain().focus().toggleItalic().run(), active: e => e.isActive('italic') },
    { label: 'S', title: 'Strikethrough', run: e => e.chain().focus().toggleStrike().run(), active: e => e.isActive('strike') },
    { label: '<>', title: 'Inline code', run: e => e.chain().focus().toggleCode().run(), active: e => e.isActive('code') },
    ...([1, 2, 3] as const).map(level => ({
      label: `H${level}`,
      title: `Heading ${level}`,
      run: (e: Editor) => e.chain().focus().toggleHeading({ level }).run(),
      active: (e: Editor) => e.isActive('heading', { level }),
    })),
    { label: '• List', title: 'Bullet list', run: e => e.chain().focus().toggleBulletList().run(), active: e => e.isActive('bulletList') },
    { label: '1. List', title: 'Numbered list', run: e => e.chain().focus().toggleOrderedList().run(), active: e => e.isActive('orderedList') },
    { label: 'Quote', title: 'Quote', run: e => e.chain().focus().toggleBlockquote().run(), active: e => e.isActive('blockquote') },
    { label: 'Code', title: 'Code block', run: e => e.chain().focus().toggleCodeBlock().run(), active: e => e.isActive('codeBlock') },
    { label: 'Rule', title: 'Horizontal rule', run: e => e.chain().focus().setHorizontalRule().run() },
    {
      label: 'Link',
      title: 'Link',
      run: e => {
        const href = prompt('Link address', e.getAttributes('link').href ?? 'https://')
        if (href === null) return
        if (href === '') e.chain().focus().unsetLink().run()
        else e.chain().focus().extendMarkRange('link').setLink({ href }).run()
      },
      active: e => e.isActive('link'),
    },
  ]

  /** Which buttons are pressed, by title. Read again after everything the editor does. */
  let pressed = $state.raw<Record<string, boolean>>({})

  $effect(() => {
    if (!editor) return
    const current = editor
    let shown = true
    // The editor can change while Svelte is updating the screen, so its state is read afterwards.
    const read = () =>
      queueMicrotask(() => {
        if (!shown || current.isDestroyed) return
        pressed = Object.fromEntries(buttons.flatMap(b => (b.active ? [[b.title, b.active(current)]] : [])))
      })
    read()
    current.on('transaction', read)
    return () => {
      shown = false
      current.off('transaction', read)
    }
  })
</script>

<div class="toolbar" role="toolbar" aria-label="Formatting">
  {#if editor}
    {#each buttons as button}
      <!-- Keeping the mouse press from the button leaves the selection in the editor. -->
      <button
        type="button"
        title={button.title}
        aria-pressed={button.active ? pressed[button.title] ?? false : undefined}
        onmousedown={event => event.preventDefault()}
        onclick={() => button.run(editor)}
      >
        {button.label}
      </button>
    {/each}
  {/if}
  <span class="end">{@render children?.()}</span>
</div>

<style>
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-4);
    border-bottom: var(--line-width) solid var(--color-line);
  }

  .end {
    margin-left: auto;
  }
</style>
