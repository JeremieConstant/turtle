<script lang="ts">
  /* Every result for a query. The query lives in the address, so a search can be shared. */
  import SearchField from '../components/SearchField.svelte'
  import SearchResults from '../components/SearchResults.svelte'
  import { liveSearch } from '../lib/liveSearch.svelte'
  import { router } from '../lib/router.svelte'
  import { searchAddress } from '../lib/routes'

  /** The query in the address. */
  const { query }: { query: string } = $props()

  let value = $state('')

  // The field follows the address, as when going back, unless the address only says what was just typed.
  $effect(() => {
    if (query !== value.trim()) value = query
  })

  /** What is typed goes into the field and into the address in one step, so neither is ever behind the other. */
  function type(typed: string) {
    value = typed
    router.rewrite(searchAddress(typed.trim()))
  }

  const search = liveSearch(() => ({ query }))

  $effect(() => {
    document.title = 'Search · Turtle'
  })
</script>

<main>
  <SearchField
    bind:value={() => value, type}
    onkeydown={event => {
      // Not every browser clears a search field on Escape by itself.
      if (event.key === 'Escape') type('')
    }}
  />

  {#if query.trim() && search.error}
    <p class="count" role="alert">{search.error}</p>
  {:else if query.trim() && search.results}
    {@const results = search.results}
    <p class="count" role="status">
      {#if results.length === 0}
        Nothing matches “{search.answered}”.
      {:else}
        {results.length} {results.length === 1 ? 'result' : 'results'} for “{search.answered}”
      {/if}
    </p>
    <SearchResults {results} />
  {/if}
</main>

<style>
  main {
    max-width: calc(var(--width-text) + 2 * var(--space-6));
    margin: 0 auto;
    padding: var(--space-10) var(--space-6) var(--space-16);
  }

  .count {
    margin: var(--space-6) var(--space-3) var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  @media (max-width: 800px) {
    main {
      padding: var(--space-6) var(--space-4) var(--space-12);
    }
  }
</style>
