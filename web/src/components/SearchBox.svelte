<script lang="ts">
  import { call, OperationError } from '../lib/ops'
  import type { SearchResult } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { latestOnly, place } from '../lib/search'

  /** The Area being looked at, if any; the search covers only it unless widened. */
  const { areaId }: { areaId?: string } = $props()

  /** How long typing must pause before a search is sent, in milliseconds. */
  const PAUSE = 120

  let query = $state('')
  let everywhere = $state(false)
  let open = $state(false)
  let results = $state.raw<SearchResult[] | null>(null)
  let error = $state<string | null>(null)

  const inArea = $derived(areaId !== undefined && !everywhere)
  const ask = latestOnly((query: string, area_id?: string) => call('search', { query, area_id }))

  // Results follow what is typed, and the scope.
  $effect(() => {
    const text = query.trim()
    const area_id = inArea ? areaId : undefined
    if (!text) {
      results = null
      error = null
      return
    }
    const timer = setTimeout(async () => {
      try {
        const answer = await ask(text, area_id)
        if (answer) {
          results = answer.results
          error = null
        }
      } catch (caught) {
        error = caught instanceof OperationError ? caught.message : 'Searching failed. Try again.'
      }
    }, PAUSE)
    return () => clearTimeout(timer)
  })

  function closeWhenLeft(event: FocusEvent) {
    const container = event.currentTarget as HTMLElement
    if (!container.contains(event.relatedTarget as Node | null)) open = false
  }

  function go(event: MouseEvent, address: string) {
    router.follow(event, address)
    if (event.defaultPrevented) {
      open = false
      query = ''
    }
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
  {#if open && query.trim() && (results || error)}
    <div class="results">
      {#if error}
        <p class="message">{error}</p>
      {:else if results && results.length === 0}
        <p class="message">Nothing found.</p>
      {:else if results}
        <ul>
          {#each results as result (result.page_id)}
            {@const address = pageAddress(result.page_id, result.title)}
            <li>
              <a href={address} onclick={event => go(event, address)}>
                <span class="title">{result.title}</span>
                <span class="place">{place(result)}</span>
                {#if result.snippet.length > 0}
                  <span class="snippet"
                    >{#each result.snippet as piece, index (index)}{#if piece.highlighted}<mark>{piece.text}</mark
                        >{:else}{piece.text}{/if}{/each}</span
                  >
                {/if}
              </a>
            </li>
          {/each}
        </ul>
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
    width: var(--width-search);
    max-width: 100%;
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
    z-index: var(--layer-popover);
    width: var(--width-search-results);
    max-width: var(--width-popover-max);
    margin-top: var(--space-1);
    background: var(--color-surface);
    border: var(--line-width) solid var(--color-line);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-popover);
  }

  ul {
    margin: 0;
    padding: var(--space-1) 0;
    list-style: none;
  }

  a {
    display: flex;
    flex-direction: column;
    gap: var(--space-hair);
    padding: var(--space-2) var(--space-3);
    color: inherit;
    text-decoration: none;
  }

  a:hover,
  a:focus-visible {
    background: var(--color-bg);
  }

  .title {
    font-weight: var(--weight-strong);
  }

  .place {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .snippet {
    font-size: var(--text-s);
    line-height: var(--leading-tight);
  }

  mark {
    background: var(--color-highlight);
    color: inherit;
  }

  .message {
    margin: 0;
    padding: var(--space-3);
    color: var(--color-muted);
  }
</style>
