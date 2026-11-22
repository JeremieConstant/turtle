<script lang="ts">
  /*
   * The formatting toolbar above the content, for anyone who does not know
   * "/" or the shortcuts. It stays under the Page's bar while you scroll and
   * always shows what the cursor is in. Its buttons never take the focus
   * away from the text.
   */
  import type { Editor } from '@tiptap/core'
  import Icon from '../components/Icon.svelte'
  import { BLOCKS, INSERTS, LISTS, MARKS, STYLES, type Format, type Pick } from './format'

  let { editor, version, pick }: { editor: Editor; version: number; pick: (what: Pick) => void } = $props()

  // Read again after every transaction (version), so active states follow the cursor.
  const current = $derived.by(() => {
    void version
    const style = STYLES.find((s) => s.active?.(editor)) ?? null
    const active = new Set([...MARKS, ...LISTS, ...BLOCKS].filter((f) => f.active?.(editor)).map((f) => f.id))
    const disabled = new Set([...MARKS, ...INSERTS].filter((f) => f.can && !f.can(editor)).map((f) => f.id))
    return { style, active, disabled, undo: editor.can().undo(), redo: editor.can().redo() }
  })

  let styleMenu = $state<HTMLDivElement>()

  const mac = /Mac|iP(hone|ad)/.test(navigator.platform)
  const keep = (event: MouseEvent) => event.preventDefault()
  const tip = (f: Format) => (f.shortcut ? `${f.label}  ${f.shortcut}` : f.label)

  /* Unlike the other buttons, this one takes the focus like any menu button:
     the open menu moves it to its items, Escape returns it here, and
     choosing a style hands it back to the text. */
  function ontoggle(event: ToggleEvent) {
    if (event.newState === 'open')
      requestAnimationFrame(() => (styleMenu?.querySelector<HTMLButtonElement>('.on') ?? styleMenu?.querySelector('button'))?.focus())
  }

  function menuKeys(event: KeyboardEvent) {
    if (event.key === 'Escape') return styleMenu?.hidePopover()
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
    event.preventDefault()
    const items = [...styleMenu!.querySelectorAll<HTMLButtonElement>('button')]
    const at = items.indexOf(document.activeElement as HTMLButtonElement)
    items[(at + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length].focus()
  }

  function chooseStyle(f: Format) {
    styleMenu?.hidePopover()
    f.run(editor, pick)
  }
</script>

{#snippet button(f: Format)}
  <button
    class="tool"
    class:on={current.active.has(f.id)}
    aria-pressed={f.active ? current.active.has(f.id) : undefined}
    aria-label={f.label}
    title={tip(f)}
    disabled={current.disabled.has(f.id)}
    onmousedown={keep}
    onclick={() => f.run(editor, pick)}
  >
    <Icon name={f.icon} />
  </button>
{/snippet}

<div class="toolbar" role="toolbar" aria-label="Formatting">
  <button class="tool" aria-label="Undo" title={mac ? 'Undo  ⌘Z' : 'Undo  Ctrl+Z'} disabled={!current.undo} onmousedown={keep} onclick={() => editor.chain().focus().undo().run()}>
    <Icon name="undo" />
  </button>
  <button class="tool" aria-label="Redo" title={mac ? 'Redo  ⌘⇧Z' : 'Redo  Ctrl+Shift+Z'} disabled={!current.redo} onmousedown={keep} onclick={() => editor.chain().focus().redo().run()}>
    <Icon name="redo" />
  </button>

  <span class="sep" aria-hidden="true"></span>

  <button
    class="style"
    popovertarget="text-style"
    style:anchor-name="--text-style"
    aria-label="Text style: {current.style?.label ?? 'Mixed'}"
    title="Text style"
  >
    <span>{current.style?.label ?? 'Text'}</span>
    <Icon name="caret" size={14} />
  </button>
  <div class="menu" id="text-style" popover role="menu" tabindex="-1" bind:this={styleMenu} style:position-anchor="--text-style" {ontoggle} onkeydown={menuKeys}>
    {#each STYLES as f (f.id)}
      <button role="menuitemradio" aria-checked={current.style?.id === f.id} class:on={current.style?.id === f.id} onmousedown={keep} onclick={() => chooseStyle(f)}>
        <Icon name={f.icon} />
        <span class="label">{f.label}</span>
        {#if f.shortcut}<kbd>{f.shortcut}</kbd>{/if}
      </button>
    {/each}
  </div>

  <span class="sep" aria-hidden="true"></span>
  {#each MARKS as f (f.id)}{@render button(f)}{/each}
  <span class="sep" aria-hidden="true"></span>
  {#each LISTS as f (f.id)}{@render button(f)}{/each}
  <span class="sep" aria-hidden="true"></span>
  {#each BLOCKS as f (f.id)}{@render button(f)}{/each}
  <span class="sep" aria-hidden="true"></span>
  {#each INSERTS as f (f.id)}{@render button(f)}{/each}
</div>

<style>
  .toolbar {
    position: sticky;
    top: var(--height-bar);
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0 calc(-1 * var(--space-2)) var(--space-6);
    padding: var(--space-1) var(--space-2);
    border-top: var(--line) solid var(--color-line);
    border-bottom: var(--line) solid var(--color-line);
    background: rgb(255 255 255 / 0.9);
    backdrop-filter: blur(12px) saturate(180%);
    /* Narrow columns get a second row rather than hidden buttons. */
    flex-wrap: wrap;
  }

  .tool {
    flex: none;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--color-text-2);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out),
      transform var(--dur-instant) var(--ease-out);
  }

  .tool:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .tool:active:not(:disabled) {
    transform: scale(0.92);
  }

  /* What the cursor is in reads as pressed, in the same grey as the open Page. */
  .tool.on {
    background: var(--color-selected);
    color: var(--color-text);
  }

  .tool:disabled {
    color: var(--color-faint);
    cursor: default;
  }

  .sep {
    flex: none;
    width: var(--line);
    height: 18px;
    margin: 0 var(--space-1);
    background: var(--color-line);
  }

  .style {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-1);
    width: 108px;
    height: 30px;
    padding: 0 var(--space-1) 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--color-text);
    font-size: var(--text-m);
    transition: background var(--dur-fast) var(--ease-out);
  }

  .style:hover {
    background: var(--color-hover);
  }

  .style :global(svg) {
    color: var(--color-muted);
  }

  .menu {
    position: fixed;
    position-area: bottom span-right;
    margin-top: var(--space-1);
    min-width: 220px;
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 32px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: var(--color-text);
    font-size: var(--text-m);
    text-align: left;
  }

  .menu button :global(svg) {
    color: var(--color-muted);
  }

  .menu button:hover,
  .menu button:focus-visible {
    background: var(--color-hover);
    outline: none;
  }

  .menu button.on {
    font-weight: var(--weight-medium);
  }

  .label {
    flex: 1;
  }

  kbd {
    color: var(--color-faint);
    font-family: var(--font-body);
    font-size: var(--text-xs);
  }
</style>
