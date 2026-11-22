<script lang="ts">
  /*
   * The deleted Areas, for a Workspace Admin: once deleted, an Area is out of
   * reach of its own Admins, so this is the one place it is restored from.
   */
  import { navigation } from '../lib/navigation.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { DeletedArea } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { areaAddress } from '../lib/routes'
  import { exactTime, relativeTime } from '../lib/time'
  import { daysLeft, deletedBy, endsSoon } from '../lib/trash'

  let areas = $state.raw<DeletedArea[] | null>(null)
  let loadError = $state<string | null>(null)
  let busy = $state(false)
  let error = $state<string | null>(null)
  const now = Date.now() / 1000

  call('list_deleted_areas', {})
    .then(list => (areas = list.areas))
    .catch(caught => {
      loadError =
        caught instanceof OperationError && caught.kind === 'not_permitted'
          ? 'Only a Workspace Admin can restore deleted Areas.'
          : 'The deleted Areas could not be loaded. Reload the page to try again.'
    })

  $effect(() => {
    document.title = 'Deleted Areas · Turtle'
  })

  async function restore(area: DeletedArea) {
    busy = true
    error = null
    try {
      const restored = await call('restore_area', { id: area.id })
      areas = areas?.filter(known => known.id !== area.id) ?? null
      await navigation.refreshAreas()
      router.navigate(areaAddress(restored.id, restored.name))
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

<main class="screen">
  <h1>Deleted Areas</h1>
  {#if loadError}
    <p class="muted section">{loadError}</p>
  {:else if areas !== null}
    <p class="muted section">A deleted Area is kept with everything in it for 30 days, then it is gone for good. Restoring brings it back for everyone who had a Role on it.</p>

    {#if error}
      <p class="error section" role="alert">{error}</p>
    {/if}

    {#if areas.length > 0}
      <ul class="list section">
        {#each areas as area (area.id)}
          <li>
            <div class="item">
              <span class="item-name">{area.name}</span>
              <small class="hint">
                {deletedBy(area.deleted_by)},
                <time datetime={new Date(area.deleted_at * 1000).toISOString()} title={exactTime(area.deleted_at)}>{relativeTime(area.deleted_at, now)}</time>
                · <span class:soon={endsSoon(area.days_left)}>{daysLeft(area.days_left)}</span>
              </small>
            </div>
            <div class="controls">
              <button type="button" class="btn" disabled={busy} onclick={() => restore(area)}>Restore</button>
            </div>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted section">No Area has been deleted.</p>
    {/if}
  {/if}
</main>

<style>
  .soon {
    color: var(--color-danger);
  }
</style>
