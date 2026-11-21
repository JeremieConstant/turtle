<script lang="ts">
  /* Full search: the query lives in the address, so a search can be shared. */
  import Hits from '../components/Hits.svelte'
  import Icon from '../components/Icon.svelte'
  import { search } from '../lib/mock/ops.svelte'
  import { router } from '../lib/router.svelte'
  import { searchAddress } from '../lib/routes'

  let { query }: { query: string } = $props()

  let value = $state('')
  $effect(() => {
    value = query
  })
  const hits = $derived(value.trim() ? search({ query: value, limit: 50 }) : [])

  function oninput() {
    router.replace(searchAddress(value.trim()))
  }
</script>

<div class="screen">
  <div class="column">
    <form class="search" role="search" onsubmit={(e) => e.preventDefault()}>
      <Icon name="search" size={18} />
      <!-- svelte-ignore a11y_autofocus -->
      <input type="search" bind:value {oninput} placeholder="Search every Area" aria-label="Search every Area" autofocus />
    </form>

    {#if value.trim()}
      <p class="count" aria-live="polite">
        {hits.length ? `${hits.length === 50 ? '50+' : hits.length} ${hits.length === 1 ? 'Page' : 'results'} for “${value.trim()}”` : `Nothing matches “${value.trim()}”`}
      </p>
      {#key value.trim()}
        <Hits {hits} />
      {/key}
    {/if}
  </div>
</div>

<style>
  .screen {
    padding: var(--space-10) var(--space-6) var(--space-16);
  }

  .column {
    max-width: var(--width-text);
    margin: 0 auto;
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 48px;
    padding: 0 var(--space-4);
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-l);
    color: var(--color-muted);
    transition: border-color var(--dur-fast) var(--ease-out), box-shadow var(--dur-fast) var(--ease-out);
  }

  .search:focus-within {
    border-color: var(--color-focus);
    box-shadow: 0 0 0 4px var(--color-focus-soft);
  }

  input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--color-text);
    font-size: var(--text-l);
  }

  .count {
    margin: var(--space-6) var(--space-3) var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .column :global(.hits) {
    margin: 0 calc(-1 * var(--space-0, 0px));
  }

  @media (max-width: 800px) {
    .screen {
      padding: var(--space-16) var(--space-4) var(--space-12);
    }
  }
</style>
