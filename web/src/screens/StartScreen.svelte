<script lang="ts">
  /* The start screen answers one question first: where is what I need. */
  import SearchField from '../components/SearchField.svelte'
  import SearchResults from '../components/SearchResults.svelte'
  import { liveSearch } from '../lib/liveSearch.svelte'
  import { router } from '../lib/router.svelte'
  import { pageAddress, searchAddress } from '../lib/routes'
  import { moveMark } from '../lib/search'

  /** How many results show under the field; the rest are on the search screen. */
  const SHOWN = 6
  const LIST = 'start-results'

  let query = $state('')
  let marked = $state(0)
  let focused = $state(false)

  const search = liveSearch(() => ({ query, limit: SHOWN }))
  const text = $derived(query.trim())
  // After a failed search the message stands in for the list, so there is nothing to mark or open.
  const results = $derived(search.error ? [] : (search.results ?? []))
  const open = $derived(focused && text !== '' && (search.results !== null || search.error !== null))
  const everything = $derived(searchAddress(text))

  $effect(() => {
    document.title = 'Turtle'
  })

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      marked = moveMark(marked, results.length, event.key)
    } else if (event.key === 'Escape') {
      query = ''
    } else if (event.key === 'Enter' && text) {
      // The results on the screen are behind while the answer to what was just typed is on its way.
      const result = search.answered === text ? results[marked] : undefined
      // Shift, or an answer that has not arrived yet, leads to every result instead.
      router.navigate(result && !event.shiftKey ? pageAddress(result.page_id, result.title) : everything)
    }
  }

  function leave(event: FocusEvent) {
    const container = event.currentTarget as HTMLElement
    if (!container.contains(event.relatedTarget as Node | null)) focused = false
  }
</script>

<main>
  <div class="search" onfocusin={() => (focused = true)} onfocusout={leave}>
    <SearchField
      large
      bind:value={query}
      role="combobox"
      aria-expanded={open}
      aria-controls={LIST}
      aria-autocomplete="list"
      aria-activedescendant={open && results[marked] ? `${LIST}-${marked}` : undefined}
      oninput={() => (marked = 0)}
      {onkeydown}
    />
    {#if open}
      <!-- A press on a result must not take the focus from the field, or the list would be gone before the click. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="results" onmousedown={event => event.preventDefault()}>
        {#if search.error}
          <p class="message" role="alert">{search.error}</p>
        {:else if results.length === 0}
          <p class="message" role="status">Nothing matches “{search.answered}”.</p>
        {:else}
          <SearchResults {results} compact listbox={LIST} {marked} />
          <a class="all" href={everything} onclick={event => router.follow(event, everything)}>
            All results for “{text}”
          </a>
        {/if}
      </div>
    {/if}
  </div>
</main>

<style>
  main {
    max-width: calc(var(--width-start) + 2 * var(--space-6));
    margin: 0 auto;
    padding: var(--space-start-top) var(--space-6) var(--space-16);
  }

  .search {
    position: relative;
  }

  .results {
    position: absolute;
    top: calc(100% + var(--space-2));
    left: 0;
    right: 0;
    z-index: var(--layer-menu);
    padding: var(--space-1);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-l);
    background: var(--color-surface);
    box-shadow: var(--shadow-float);
    animation: results-in var(--dur-fast) var(--ease-out);
  }

  @keyframes results-in {
    from {
      opacity: 0;
      transform: translateY(calc(-1 * var(--space-1)));
    }
  }

  .all {
    display: block;
    margin-top: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border-top: var(--line) solid var(--color-line);
    color: var(--color-muted);
    font-size: var(--text-s);
    text-decoration: none;
  }

  .all:hover,
  .all:focus-visible {
    color: var(--color-text);
  }

  .message {
    margin: 0;
    padding: var(--space-3);
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  @media (max-width: 800px) {
    main {
      padding: var(--space-12) var(--space-4);
    }
  }
</style>
