<script lang="ts">
  import { tick, untrack } from 'svelte'
  import AppHeader from '../components/AppHeader.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Area } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { areaAddress } from '../lib/routes'
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

  call('get_area', { id })
    .then(found => (area = found))
    .catch(caught => {
      if (caught instanceof OperationError && caught.kind === 'not_found') missing = true
      else loadFailed = true
    })

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
  <AppHeader />
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
      <p class="muted">Your Role here: {area.role}. This Area has no Pages yet.</p>
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

  .muted {
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  .error {
    color: var(--color-danger);
  }
</style>
