<script lang="ts">
  /*
   * The formatting toolbar above the content, for anyone who does not know
   * the shortcuts. It stays under the Page's bar while the Page scrolls and
   * shows what applies where the cursor is. Its buttons never take the focus
   * away from the text.
   */
  import type { Editor } from '@tiptap/core'
  import { BLOCKS, CALLOUTS, LISTS, MARKS, STYLES, shortcut, type Format } from '../lib/format'
  import { liveSearch } from '../lib/liveSearch.svelte'
  import { menuKeys, openMenu } from '../lib/menu'
  import { pageIdOf } from '../lib/pageLink'
  import Icon from './Icon.svelte'

  const { editor }: { editor: Editor } = $props()

  type Current = { style: Format | null; active: Set<string>; undo: boolean; redo: boolean }
  let current = $state.raw<Current>({ style: null, active: new Set(), undo: false, redo: false })

  $effect(() => {
    const watched = editor
    let shown = true
    // The editor can change while Svelte is updating the screen, so its state is read afterwards.
    const read = () =>
      queueMicrotask(() => {
        if (!shown || watched.isDestroyed) return
        current = {
          style: STYLES.find(f => f.active?.(watched)) ?? null,
          active: new Set(
            [...MARKS, ...LISTS, ...BLOCKS, ...CALLOUTS, LINK].filter(f => f.active?.(watched)).map(f => f.id),
          ),
          undo: watched.can().undo(),
          redo: watched.can().redo(),
        }
      })
    read()
    watched.on('transaction', read)
    return () => {
      shown = false
      watched.off('transaction', read)
    }
  })

  /** Keeps a mouse press from taking the selection out of the text. */
  const keep = (event: MouseEvent) => event.preventDefault()
  const tip = (f: Format) => (f.shortcut ? `${f.label}  ${f.shortcut}` : f.label)

  let styleButton = $state<HTMLButtonElement>()
  let styleMenu = $state<HTMLDivElement>()

  function chooseStyle(f: Format) {
    styleMenu?.hidePopover()
    f.run(editor)
  }

  let calloutButton = $state<HTMLButtonElement>()
  let calloutMenu = $state<HTMLDivElement>()
  const inCallout = $derived(CALLOUTS.some(f => current.active.has(f.id)))

  function chooseCallout(f: Format) {
    calloutMenu?.hidePopover()
    f.run(editor)
  }

  /* A link needs an address. Inside a link the same button takes it off. */
  let linkDialog = $state<HTMLDialogElement>()
  let address = $state('')
  const LINK: Format = {
    id: 'link',
    label: 'Link',
    icon: 'link',
    active: e => e.isActive('link'),
    run: e => {
      if (e.isActive('link')) return e.chain().focus().extendMarkRange('link').unsetLink().run()
      address = ''
      linkDialog?.showModal()
    },
  }

  /* Searching for a Page by name is how a writer finds one; Page IDs are not meant to be typed. */
  let pageLinkDialog = $state<HTMLDialogElement>()
  let pageQuery = $state('')
  const found = liveSearch(() => ({ query: pageQuery, limit: 8 }))
  const PAGE_LINK: Format = {
    id: 'pageLink',
    label: 'Link to Page',
    icon: 'pagelink',
    run: () => {
      pageQuery = ''
      pageLinkDialog?.showModal()
    },
  }

  function linkPage(pageId: string) {
    pageLinkDialog?.close()
    editor.chain().focus().insertContent({ type: 'pageLink', attrs: { pageId } }).run()
  }

  /** Enter takes the best match. */
  function linkFirstPage(event: SubmitEvent) {
    event.preventDefault()
    const first = found.results?.[0]
    if (first) linkPage(first.page_id)
  }

  function setLink(event: SubmitEvent) {
    event.preventDefault()
    const href = address.trim()
    linkDialog?.close()
    if (!href) return
    // An address of a Page is a Page link, as it would be once read back from Markdown.
    const pageId = pageIdOf(href)
    if (pageId) return linkPage(pageId)
    // A bare "example.org" is meant as a web address, not a path inside Turtle.
    const full = /^([a-z][a-z0-9+.-]*:|\/|#)/i.test(href) ? href : `https://${href}`
    if (editor.state.selection.empty) {
      editor.chain().focus().insertContent({ type: 'text', text: href, marks: [{ type: 'link', attrs: { href: full } }] }).run()
    } else {
      editor.chain().focus().extendMarkRange('link').setLink({ href: full }).run()
    }
  }
</script>

{#snippet button(f: Format)}
  <button
    type="button"
    class="tool"
    class:on={current.active.has(f.id)}
    aria-pressed={f.active ? current.active.has(f.id) : undefined}
    aria-label={f.label}
    title={tip(f)}
    onmousedown={keep}
    onclick={() => f.run(editor)}
  >
    <Icon name={f.icon} />
  </button>
{/snippet}

<div class="toolbar" role="toolbar" aria-label="Formatting">
  <button
    type="button"
    class="tool"
    aria-label="Undo"
    title="Undo  {shortcut('Mod+Z')}"
    disabled={!current.undo}
    onmousedown={keep}
    onclick={() => editor.chain().focus().undo().run()}
  >
    <Icon name="undo" />
  </button>
  <button
    type="button"
    class="tool"
    aria-label="Redo"
    title="Redo  {shortcut('Mod+Shift+Z')}"
    disabled={!current.redo}
    onmousedown={keep}
    onclick={() => editor.chain().focus().redo().run()}
  >
    <Icon name="redo" />
  </button>

  <span class="separator" aria-hidden="true"></span>

  <!-- Unlike the others, this button takes the focus like any menu button:
       the menu moves it to its items, and choosing one hands it back to the text. -->
  <button
    type="button"
    class="style"
    bind:this={styleButton}
    aria-haspopup="menu"
    aria-label="Text style: {current.style?.label ?? 'Mixed'}"
    title="Text style"
    onclick={() => styleMenu && styleButton && openMenu(styleMenu, styleButton)}
  >
    <span>{current.style?.label ?? 'Text'}</span>
    <Icon name="caret" />
  </button>
  <div class="menu" popover role="menu" tabindex="-1" aria-label="Text style" bind:this={styleMenu} onkeydown={menuKeys}>
    {#each STYLES as f (f.id)}
      <button
        type="button"
        class="menu-item style-item"
        role="menuitemradio"
        aria-checked={current.style?.id === f.id}
        onclick={() => chooseStyle(f)}
      >
        <Icon name={f.icon} />
        <span class="label">{f.label}</span>
        {#if f.shortcut}<kbd>{f.shortcut}</kbd>{/if}
      </button>
    {/each}
  </div>

  <span class="separator" aria-hidden="true"></span>
  {#each MARKS as f (f.id)}{@render button(f)}{/each}
  {@render button(LINK)}
  {@render button(PAGE_LINK)}
  <span class="separator" aria-hidden="true"></span>
  {#each LISTS as f (f.id)}{@render button(f)}{/each}
  <span class="separator" aria-hidden="true"></span>
  {#each BLOCKS as f (f.id)}{@render button(f)}{/each}
  <!-- Like the text style, a menu: it takes the focus and hands it back to the text. -->
  <button
    type="button"
    class="tool"
    class:on={inCallout}
    bind:this={calloutButton}
    aria-haspopup="menu"
    aria-label="Callout"
    title="Callout"
    onclick={() => calloutMenu && calloutButton && openMenu(calloutMenu, calloutButton)}
  >
    <Icon name="callout" />
  </button>
  <div class="menu" popover role="menu" tabindex="-1" aria-label="Callout" bind:this={calloutMenu} onkeydown={menuKeys}>
    {#each CALLOUTS as f (f.id)}
      <button
        type="button"
        class="menu-item style-item callout-item {f.id}"
        role="menuitemcheckbox"
        aria-checked={current.active.has(f.id)}
        onclick={() => chooseCallout(f)}
      >
        <Icon name={f.icon} />
        <span class="label">{f.label}</span>
      </button>
    {/each}
  </div>
</div>

<dialog bind:this={linkDialog} closedby="any" onclose={() => editor.commands.focus()}>
  <form onsubmit={setLink}>
    <label for="link-address">Link address</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="link-address" bind:value={address} autofocus placeholder="https://…" />
    <div class="actions">
      <button type="button" class="btn" onclick={() => linkDialog?.close()}>Cancel</button>
      <button type="submit" class="btn primary">Add link</button>
    </div>
  </form>
</dialog>

<dialog bind:this={pageLinkDialog} closedby="any" onclose={() => editor.commands.focus()}>
  <form onsubmit={linkFirstPage}>
    <label for="page-link-query">Link to Page</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="page-link-query" type="search" bind:value={pageQuery} autofocus autocomplete="off" placeholder="Search for a Page by its title" />
    {#if found.error}
      <p class="note" role="alert">{found.error}</p>
    {:else if found.results?.length === 0}
      <p class="note">No Page matches “{found.answered}”.</p>
    {:else if found.results}
      <ul class="pages" aria-label="Matching Pages">
        {#each found.results as result (result.page_id)}
          <li>
            <button type="button" class="page" onclick={() => linkPage(result.page_id)}>
              <span class="page-title">{result.title}</span>
              <span class="page-place">{[result.area_name, ...result.path].join(' / ')}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="actions">
      <button type="button" class="btn" onclick={() => pageLinkDialog?.close()}>Cancel</button>
    </div>
  </form>
</dialog>

<style>
  .toolbar {
    position: sticky;
    top: var(--height-bar);
    z-index: 1;
    display: flex;
    /* A narrow column gets a second row rather than hidden buttons. */
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-half);
    margin: 0 calc(-1 * var(--space-2)) var(--space-6);
    padding: var(--space-1) var(--space-2);
    border-top: var(--line) solid var(--color-line);
    border-bottom: var(--line) solid var(--color-line);
    background: var(--color-bar);
    backdrop-filter: blur(var(--blur-bar)) saturate(180%);
  }

  .tool {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--size-tool);
    height: var(--size-tool);
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
    transform: scale(var(--press-scale-icon));
  }

  /* What applies at the cursor reads as pressed. */
  .tool.on {
    background: var(--color-selected);
    color: var(--color-text);
  }

  .tool:disabled {
    opacity: 1;
    color: var(--color-faint);
  }

  .separator {
    flex: none;
    width: var(--line);
    height: var(--height-tool-separator);
    margin: 0 var(--space-1);
    background: var(--color-line);
  }

  .style {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-1);
    width: var(--width-text-style);
    height: var(--size-tool);
    padding: 0 var(--space-1) 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    font-weight: var(--weight-normal);
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
    min-width: var(--width-text-style-menu);
  }

  .style-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2);
  }

  .style-item :global(svg) {
    color: var(--color-muted);
  }

  .style-item[aria-checked='true'] {
    font-weight: var(--weight-medium);
  }

  /* Each kind shows in its own colour, as the callout will. */
  .callout-item.callout-note :global(svg) {
    color: var(--color-callout-note);
  }

  .callout-item.callout-tip :global(svg) {
    color: var(--color-callout-tip);
  }

  .callout-item.callout-warning :global(svg) {
    color: var(--color-callout-warning);
  }

  .callout-item.callout-danger :global(svg) {
    color: var(--color-callout-danger);
  }

  .label {
    flex: 1;
  }

  kbd {
    color: var(--color-muted);
    font-family: var(--font-body);
    font-size: var(--text-xs);
  }

  dialog {
    width: min(var(--width-form), var(--width-floating-max));
  }

  /* There the Page's bar scrolls away, so the toolbar holds at the very top. */
  @media (max-width: 800px) {
    .toolbar {
      top: 0;
    }
  }

  form {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-4);
  }

  label {
    font-size: var(--text-s);
    font-weight: var(--weight-medium);
  }

  .note {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .pages {
    display: grid;
    gap: var(--space-half);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .page {
    display: grid;
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    text-align: left;
    transition: background var(--dur-fast) var(--ease-out);
  }

  .page:hover,
  .page:focus-visible {
    background: var(--color-hover);
  }

  .page-title {
    font-weight: var(--weight-medium);
  }

  .page-place {
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
