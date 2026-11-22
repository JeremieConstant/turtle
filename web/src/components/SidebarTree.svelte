<script lang="ts">
  /*
   * The tree of one Area in the sidebar, drawn as one flat list of the rows
   * that are visible, each indented by how deep it sits. A Folder, and a
   * Page with something beneath it, folds and unfolds.
   */
  import { untrack } from 'svelte'
  import type { Area, Entry } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { above, visibleRows } from '../lib/tree'
  import { collapsed } from '../lib/ui.svelte'
  import Icon from './Icon.svelte'

  const {
    area,
    entries,
    currentId,
  }: {
    area: Area
    /** The top of the Area's tree, in order. */
    entries: Entry[]
    /** The Page that is open on the screen, if one is. */
    currentId: string | null
  } = $props()

  const rows = $derived(visibleRows(entries, collapsed))

  // Opening a Page unfolds what it sits beneath, so its row is there to be marked.
  // Only then: what someone folds afterwards stays folded.
  $effect(() => {
    const id = currentId
    if (id) untrack(() => above(entries, id).forEach(parent => collapsed.delete(parent)))
  })

  function toggle(id: string) {
    if (collapsed.has(id)) collapsed.delete(id)
    else collapsed.add(id)
  }
</script>

<ul aria-label="Pages and Folders in {area.name}">
  {#each rows as { entry, depth, open } (entry.id)}
    {@const address = pageAddress(entry.id, entry.title)}
    <li class="row" class:current={entry.id === currentId} style:--depth={depth}>
      {#if entry.children.length > 0}
        <button
          type="button"
          class="chevron"
          class:open
          aria-expanded={open}
          aria-label={open ? `Collapse ${entry.title}` : `Expand ${entry.title}`}
          onclick={() => toggle(entry.id)}
        >
          <Icon name="chevron" size={14} />
        </button>
      {:else}
        <span class="chevron" aria-hidden="true"></span>
      {/if}
      {#if entry.kind === 'folder'}
        <!-- The chevron says whether it is open; an empty Folder has nothing to open. -->
        <button type="button" class="label" tabindex="-1" disabled={entry.children.length === 0} onclick={() => toggle(entry.id)}>
          <span class="glyph"><Icon name="folder" size={15} /></span>
          <span class="title">{entry.title}</span>
        </button>
      {:else}
        <a
          class="label"
          href={address}
          aria-current={entry.id === currentId ? 'page' : undefined}
          onclick={event => router.follow(event, address)}
        >
          <span class="glyph"></span>
          <span class="title">{entry.title}</span>
        </a>
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul {
    margin: 0;
    padding: 0 0 var(--space-2);
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    height: var(--height-row);
    padding-left: calc(var(--space-2) + var(--depth) * var(--indent));
    padding-right: var(--space-2);
    border-radius: var(--radius-s);
    color: var(--color-text-2);
    font-size: var(--text-m);
    transition: background var(--dur-fast) var(--ease-out);
  }

  .row:hover {
    background: var(--color-hover);
  }

  .row.current {
    background: var(--color-selected);
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .chevron {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--size-chevron);
    height: var(--size-chevron);
    margin-right: var(--space-half);
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--color-muted);
  }

  button.chevron:hover {
    background: var(--color-line);
    color: var(--color-text);
  }

  button.chevron :global(svg) {
    transition: transform var(--dur-base) var(--ease-out);
  }

  button.chevron.open :global(svg) {
    transform: rotate(90deg);
  }

  .label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    height: 100%;
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    text-decoration: none;
  }

  button.label:disabled {
    opacity: 1;
  }

  .glyph {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--space-4);
    margin-right: var(--space-2);
    color: var(--color-muted);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
