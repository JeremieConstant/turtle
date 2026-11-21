<script lang="ts">
  import { call } from '../lib/ops'
  import type { Entry, Kind } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import Self from './TreeEntry.svelte'
  import TitleForm, { ADD_ACTIONS } from './TitleForm.svelte'

  /** A Page or Folder in the tree of an Area, with what is beneath it. */
  const {
    entry,
    areaId,
    writable,
    onchange,
  }: {
    entry: Entry
    areaId: string
    /** Whether the viewer may add and rename. */
    writable: boolean
    /** Called after something in the tree changed, so it is loaded again. */
    onchange: () => Promise<void>
  } = $props()

  let expanded = $state(false)
  let editing = $state<'add' | 'rename' | null>(null)

  async function add(title: string, kind: Kind) {
    const input = { area_id: areaId, parent_id: entry.id, title }
    await call(kind === 'folder' ? 'create_folder' : 'create_page', input)
    editing = null
    // The new entry is shown where it went.
    expanded = true
    await onchange()
  }

  async function rename(title: string) {
    await call(entry.kind === 'page' ? 'rename_page' : 'rename_folder', { id: entry.id, title })
    editing = null
    await onchange()
  }
</script>

<li>
  <div class="row">
    {#if entry.children.length > 0}
      <button
        type="button"
        class="toggle"
        aria-expanded={expanded}
        aria-label={expanded ? `Collapse ${entry.title}` : `Expand ${entry.title}`}
        onclick={() => (expanded = !expanded)}>{expanded ? '▾' : '▸'}</button
      >
    {:else}
      <span class="toggle" aria-hidden="true"></span>
    {/if}
    {#if entry.kind === 'page'}
      <a href={pageAddress(entry.id, entry.title)} onclick={event => router.follow(event, pageAddress(entry.id, entry.title))}>{entry.title}</a>
    {:else}
      <span class="folder">{entry.title}</span>
    {/if}
    {#if writable && editing === null}
      <span class="actions">
        <button type="button" onclick={() => (editing = 'add')} aria-label={`Add beneath ${entry.title}`}>Add</button>
        <button type="button" onclick={() => (editing = 'rename')} aria-label={`Rename ${entry.title}`}>Rename</button>
      </span>
    {/if}
  </div>

  {#if editing === 'add'}
    <TitleForm
      label={`Title of the new entry beneath ${entry.title}`}
      actions={ADD_ACTIONS}
      onsubmit={add}
      oncancel={() => (editing = null)}
    />
  {:else if editing === 'rename'}
    <TitleForm
      label={`New title of ${entry.title}`}
      initial={entry.title}
      actions={[{ label: 'Save', value: 'save' }]}
      onsubmit={rename}
      oncancel={() => (editing = null)}
    />
  {/if}

  {#if expanded && entry.children.length > 0}
    <ul>
      {#each entry.children as child (child.id)}
        <Self entry={child} {areaId} {writable} {onchange} />
      {/each}
    </ul>
  {/if}
</li>

<style>
  ul {
    margin: 0;
    padding-left: var(--space-6);
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) 0;
  }

  .toggle {
    width: var(--space-6);
    padding: 0;
    border: none;
    background: none;
    color: var(--color-muted);
    text-align: center;
  }

  .folder {
    color: var(--color-muted);
    font-weight: var(--weight-strong);
  }

  .actions {
    display: flex;
    gap: var(--space-1);
    margin-left: auto;
    font-size: var(--text-s);
  }
</style>
