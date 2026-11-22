<script lang="ts">
  import { liveSearch } from '../lib/liveSearch.svelte'
  import SearchResults from './SearchResults.svelte'

  /** The Area being looked at, if any; the search covers only it unless widened. */
  const { areaId }: { areaId?: string } = $props()

  let query = $state('')
  let everywhere = $state(false)
  let open = $state(false)

  const inArea = $derived(areaId !== undefined && !everywhere)
  const search = liveSearch(() => ({ query, areaId: inArea ? areaId : undefined }))

  function closeWhenLeft(event: FocusEvent) {
    const container = event.currentTarget as HTMLElement
    if (!container.contains(event.relatedTarget as Node | null)) open = false
  }

  function picked() {
    open = false
    query = ''
  }
</script>

<div class="search" role="search" onfocusout={closeWhenLeft}>
  <input
    type="search"
    aria-label="Search"
    placeholder={inArea ? 'Search this Area' : 'Search everything'}
    bind:value={query}
    onfocus={() => (open = true)}
    oninput={() => (open = true)}
    onkeydown={event => {
      if (event.key === 'Escape') open = false
    }}
  />
  {#if areaId !== undefined}
    <label class="scope"><input type="checkbox" bind:checked={everywhere} /> Everywhere</label>
  {/if}
  {#if open && query.trim() && (search.results || search.error)}
    <div class="results">
      {#if search.error}
        <p class="message">{search.error}</p>
      {:else if search.results && search.results.length === 0}
        <p class="message">Nothing matches “{search.answered}”.</p>
      {:else if search.results}
        <SearchResults results={search.results} compact onpick={picked} />
      {/if}
    </div>
  {/if}
</div>

<style>
  .search {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  input[type='search'] {
    flex: 1;
    min-width: 0;
    height: var(--height-control);
    padding: 0 var(--space-2);
    font-size: var(--text-m);
  }

  input[type='search']:focus {
    outline: none;
    border-color: var(--color-focus);
    box-shadow: 0 0 0 var(--ring) var(--color-focus-soft);
  }

  .scope {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--text-s);
    color: var(--color-muted);
  }

  .results {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: var(--layer-menu);
    width: var(--width-search-results);
    max-width: var(--width-floating-max);
    margin-top: var(--space-1);
    padding: var(--space-1);
    background: var(--color-surface);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-float);
  }

  .message {
    margin: 0;
    padding: var(--space-3);
    color: var(--color-muted);
  }
</style>
