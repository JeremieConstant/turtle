<script lang="ts">
  /*
   * Inserts a link to another Page (found through search) or an image (by
   * its address). Both are things Markdown can hold: [title](/p/id/title)
   * and ![alt](src).
   */
  import type { Editor } from '@tiptap/core'
  import { search } from '../lib/mock/ops.svelte'
  import { pageAddress } from '../lib/routes'

  let { editor }: { editor: Editor | null } = $props()

  let dialog: HTMLDialogElement
  let mode = $state<'page' | 'image'>('page')
  let value = $state('')
  let active = $state(0)
  const hits = $derived(mode === 'page' ? search({ query: value, limit: 8 }).filter((h) => h.kind === 'page') : [])

  export function open(next: 'page' | 'image') {
    mode = next
    value = ''
    active = 0
    dialog.showModal()
  }

  function linkTo(hit: { id: string; title: string }) {
    editor
      ?.chain()
      .focus()
      .insertContent({ type: 'text', text: hit.title, marks: [{ type: 'link', attrs: { href: pageAddress(hit.id, hit.title) } }] })
      .insertContent(' ')
      .run()
    dialog.close()
  }

  function submit(event: SubmitEvent) {
    event.preventDefault()
    if (mode === 'page') {
      if (hits[active]) linkTo(hits[active])
      return
    }
    if (!value.trim()) return
    editor?.chain().focus().setImage({ src: value.trim(), alt: '' }).run()
    dialog.close()
  }

  function onkeydown(event: KeyboardEvent) {
    if (mode !== 'page' || !hits.length) return
    if (event.key === 'ArrowDown') (active = (active + 1) % hits.length), event.preventDefault()
    if (event.key === 'ArrowUp') (active = (active - 1 + hits.length) % hits.length), event.preventDefault()
  }
</script>

<dialog bind:this={dialog} closedby="any" onclose={() => editor?.commands.focus()}>
  <form onsubmit={submit}>
    <!-- svelte-ignore a11y_autofocus -->
    <input
      bind:value
      autofocus
      {onkeydown}
      oninput={() => (active = 0)}
      placeholder={mode === 'page' ? 'Find a Page to link to' : 'Image address, https://…'}
      aria-label={mode === 'page' ? 'Find a Page' : 'Image address'}
    />
    {#if mode === 'page' && hits.length}
      <ul role="listbox">
        {#each hits as hit, i (hit.id)}
          <li>
            <button type="button" class:active={i === active} onclick={() => linkTo(hit)} onmouseenter={() => (active = i)}>
              <span>{hit.title}</span>
              <span class="where">{hit.area.name}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </form>
</dialog>

<style>
  dialog {
    width: min(480px, calc(100vw - 2 * var(--space-4)));
    margin-top: 18vh;
  }

  dialog[open] {
    animation: pop var(--dur-base) var(--ease-out);
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(4px) scale(0.98);
    }
  }

  input {
    width: 100%;
    height: 48px;
    padding: 0 var(--space-4);
    border: 0;
    outline: 0;
    font-size: var(--text-base);
  }

  ul {
    margin: 0;
    padding: var(--space-1);
    border-top: var(--line) solid var(--color-line);
    list-style: none;
  }

  button {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    width: 100%;
    height: 34px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    font-size: var(--text-m);
    text-align: left;
    align-items: center;
  }

  button.active {
    background: var(--color-hover);
  }

  .where {
    color: var(--color-muted);
    font-size: var(--text-s);
  }
</style>
