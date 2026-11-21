<script lang="ts">
  import AppHeader from '../components/AppHeader.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Area } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { areaAddress } from '../lib/routes'

  let areas = $state.raw<Area[] | null>(null)
  let loadFailed = $state(false)
  let name = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)

  call('list_areas', {})
    .then(list => (areas = list.areas))
    .catch(() => (loadFailed = true))

  $effect(() => {
    document.title = 'Areas · Turtle'
  })

  function open(event: MouseEvent, area: Area) {
    event.preventDefault()
    router.navigate(areaAddress(area.id, area.name))
  }

  async function create(event: SubmitEvent) {
    event.preventDefault()
    busy = true
    error = null
    try {
      const area = await call('create_area', { name })
      router.navigate(areaAddress(area.id, area.name))
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

<AppHeader />

<main>
  <h1>Areas</h1>
  {#if loadFailed}
    <p class="muted">The Areas could not be loaded. Reload the page to try again.</p>
  {:else if areas === null}
    <!-- Nothing until the list is here. -->
  {:else if areas.length === 0}
    <p class="muted">You cannot see any Areas yet. Create one below, or ask an Area Admin to give you a Role on theirs.</p>
  {:else}
    <ul>
      {#each areas as area (area.id)}
        <li>
          <a href={areaAddress(area.id, area.name)} onclick={event => open(event, area)}>{area.name}</a>
          <span class="muted">{area.role}</span>
        </li>
      {/each}
    </ul>
  {/if}

  <form onsubmit={create}>
    <h2>New Area</h2>
    <p class="muted">Only you can see a new Area until you give others a Role on it.</p>
    <div class="row">
      <input aria-label="Name of the new Area" placeholder="Name" required autocomplete="off" bind:value={name} />
      <button type="submit" disabled={busy}>Create Area</button>
    </div>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </form>
</main>

<style>
  main {
    max-width: var(--width-form);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  h1,
  h2 {
    line-height: var(--leading-tight);
  }

  h2 {
    margin-bottom: 0;
    font-size: var(--text-base);
  }

  ul {
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) 0;
    border-bottom: var(--line-width) solid var(--color-line);
  }

  .muted {
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  .row {
    display: flex;
    gap: var(--space-2);
  }

  input {
    flex: 1;
    min-width: 0;
  }

  .error {
    color: var(--color-danger);
  }
</style>
