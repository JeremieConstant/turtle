<script lang="ts">
  import { tick, untrack } from 'svelte'
  import Grants from '../components/Grants.svelte'
  import TitleForm, { ADD_ACTIONS } from '../components/TitleForm.svelte'
  import TreeEntry from '../components/TreeEntry.svelte'
  import { toast } from 'svelte-sonner'
  import { navigation } from '../lib/navigation.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Area, Kind } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { areaAddress } from '../lib/routes'
  import { createEntry } from '../lib/tree'
  import { ui } from '../lib/ui.svelte'
  import NotFoundScreen from './NotFoundScreen.svelte'

  // The screen is created anew for every Area, so the ID never changes underneath it.
  const props: { id: string } = $props()
  const id = untrack(() => props.id)

  let area = $state.raw<Area | null>(null)
  /** Set when the Area does not exist or the caller may not read it; the two look the same. */
  let missing = $state(false)
  let loadFailed = $state(false)

  let renaming = $state(false)
  let nameInput = $state<HTMLInputElement | null>(null)
  let name = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)

  /** The top of the tree, in order: the same one the sidebar shows. */
  const entries = $derived(navigation.tree(id) ?? null)
  const writable = $derived(area !== null && area.role !== 'read')

  // The sidebar shows the tree of the Area that is on the screen.
  ui.openArea = id

  async function fetchTree() {
    area = (await navigation.loadTree(id)).area
  }

  fetchTree().catch(caught => {
    if (caught instanceof OperationError && caught.kind === 'not_found') missing = true
    else loadFailed = true
  })

  /** Loads the tree again after something in it changed. The change itself already happened. */
  async function reload() {
    try {
      await fetchTree()
    } catch (caught) {
      // Access may have been lost in the meantime; that looks like any other missing Area.
      if (caught instanceof OperationError && caught.kind === 'not_found') missing = true
      else toast.error('The change was saved, but the tree could not be loaded again. Reload the page to see it.')
    }
  }

  async function addAtTop(title: string, kind: Kind) {
    await createEntry(kind, { area_id: id, title })
    await reload()
  }

  /** A Grant changed, and with it perhaps which Areas the person can read and what they may do there. */
  async function rolesChanged() {
    await Promise.all([reload(), navigation.refreshAreas()])
  }

  $effect(() => {
    if (area) document.title = `${area.name} · Turtle`
  })

  $effect(() => {
    if (!area) return
    // An address with an outdated or missing name is corrected in place (ADR-0007).
    const address = areaAddress(area.id, area.name)
    if (location.pathname !== address) router.replace(address)
  })

  async function startRenaming() {
    name = area?.name ?? ''
    error = null
    renaming = true
    // The current name is selected, so typing replaces it.
    await tick()
    nameInput?.select()
  }

  async function rename(event: SubmitEvent) {
    event.preventDefault()
    busy = true
    error = null
    try {
      area = await call('rename_area', { id, name })
      renaming = false
      void navigation.refreshAreas()
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

{#if missing}
  <NotFoundScreen />
{:else}
  <main>
    {#if loadFailed}
      <p class="muted">This Area could not be loaded. Reload the page to try again.</p>
    {:else if area}
      {#if renaming}
        <form onsubmit={rename}>
          <div class="row">
            <input aria-label="Name of the Area" required autocomplete="off" bind:this={nameInput} bind:value={name} />
            <button type="submit" disabled={busy}>Save</button>
            <button type="button" onclick={() => (renaming = false)}>Cancel</button>
          </div>
          {#if error}
            <p class="error" role="alert">{error}</p>
          {/if}
        </form>
      {:else}
        <div class="row">
          <h1>{area.name}</h1>
          {#if area.role === 'admin'}
            <button type="button" onclick={startRenaming}>Rename</button>
          {/if}
        </div>
      {/if}
      <p class="muted">Your Role here: {area.role}.</p>
      {#if entries && entries.length > 0}
        <ul class="tree">
          {#each entries as entry (entry.id)}
            <TreeEntry {entry} areaId={id} {writable} onchange={reload} />
          {/each}
        </ul>
      {:else if entries}
        <p class="muted">This Area has no Pages yet.</p>
      {/if}
      {#if writable}
        <section>
          <h2>Add to the top of this Area</h2>
          <TitleForm label="Title of the new Page or Folder" actions={ADD_ACTIONS} onsubmit={addAtTop} />
        </section>
      {/if}
      {#if area.role === 'admin'}
        <!-- Created anew whenever the Role changes, so it never shows a stale list. -->
        {#key area.role}
          <Grants areaId={id} onrolechange={rolesChanged} />
        {/key}
      {/if}
    {/if}
  </main>
{/if}

<style>
  main {
    max-width: var(--width-editor);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  h1 {
    margin: 0;
    line-height: var(--leading-tight);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  input {
    flex: 1;
    min-width: 0;
  }

  .tree {
    margin: var(--space-4) 0;
    padding: 0;
    list-style: none;
  }

  h2 {
    margin-bottom: 0;
    font-size: var(--text-base);
    line-height: var(--leading-tight);
  }

  .muted {
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  .error {
    color: var(--color-danger);
  }
</style>
