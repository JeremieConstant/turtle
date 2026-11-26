<script lang="ts">
  /*
   * The search at the top of the sidebar, on every screen. Looking at an Area
   * or one of its Pages, it searches that Area first; the switch above the
   * results widens it to every Area. ⌘K or / brings the focus here.
   */
  import { liveSearch } from '../lib/liveSearch.svelte'
  import type { Area } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress, searchAddress } from '../lib/routes'
  import { moveMark } from '../lib/search'
  import Icon from './Icon.svelte'
  import SearchResults from './SearchResults.svelte'

  /** The Area being looked at, if any; the search covers only it unless widened. */
  const { area }: { area?: Area } = $props()

  const LIST = 'sidebar-results'
  const mac = /Mac|iPhone|iPad/.test(navigator.platform)

  let query = $state('')
  let everywhere = $state(false)
  let focused = $state(false)
  let marked = $state(0)

  // Another Area starts narrow again.
  $effect(() => {
    void area?.id
    everywhere = false
  })

  const inArea = $derived(area !== undefined && !everywhere)
  const search = liveSearch(() => ({ query, areaId: inArea ? area?.id : undefined }))
  const text = $derived(query.trim())
  const results = $derived(search.error ? [] : (search.results ?? []))
  const open = $derived(focused && text !== '' && (search.results !== null || search.error !== null))
  const everything = $derived(searchAddress(text))

  function leave(event: FocusEvent) {
    const container = event.currentTarget as HTMLElement
    if (!container.contains(event.relatedTarget as Node | null)) focused = false
  }

  function picked() {
    focused = false
    query = ''
    ;(document.activeElement as HTMLElement | null)?.blur()
  }

  function widen(wide: boolean) {
    everywhere = wide
    marked = 0
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      marked = moveMark(marked, results.length, event.key)
    } else if (event.key === 'Escape') {
      if (query) query = ''
      else (event.currentTarget as HTMLInputElement).blur()
    } else if (event.key === 'Enter' && text) {
      // The results on the screen are behind while the answer to what was just typed is on its way.
      const result = search.answered === text ? results[marked] : undefined
      // Shift, or an answer that has not arrived yet, leads to every result instead.
      router.navigate(result && !event.shiftKey ? pageAddress(result.page_id, result.title) : everything)
      picked()
    }
  }
</script>

<div class="search" role="search" onfocusin={() => (focused = true)} onfocusout={leave}>
  <div class="field">
    <Icon name="search" size={14} />
    <input
      type="search"
      placeholder={inArea ? `Search ${area?.name}` : 'Search'}
      aria-label={inArea ? `Search ${area?.name}` : 'Search every Area'}
      autocomplete="off"
      role="combobox"
      aria-expanded={open}
      aria-controls={LIST}
      aria-autocomplete="list"
      aria-activedescendant={open && results[marked] ? `${LIST}-${marked}` : undefined}
      bind:value={query}
      oninput={() => (marked = 0)}
      {onkeydown}
    />
    {#if !focused && !query}
      <kbd aria-hidden="true">{mac ? '⌘K' : 'Ctrl K'}</kbd>
    {/if}
  </div>
  {#if open}
    <!-- A press in the list must not take the focus from the field, or the list would be gone before the click. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="results" onmousedown={event => event.preventDefault()}>
      {#if area}
        <div class="scope" role="group" aria-label="Where to search">
          <button type="button" aria-pressed={!everywhere} onclick={() => widen(false)}>
            <span class="scope-name">{area.name}</span>
          </button>
          <button type="button" aria-pressed={everywhere} onclick={() => widen(true)}>Every Area</button>
        </div>
      {/if}
      {#if search.error}
        <p class="message" role="alert">{search.error}</p>
      {:else if results.length === 0}
        <p class="message" role="status">Nothing matches “{search.answered}”.</p>
      {:else}
        <SearchResults {results} compact listbox={LIST} {marked} onpick={picked} />
      {/if}
      {#if !search.error}
        <a class="all" href={everything} onclick={event => (router.follow(event, everything), picked())}>
          {inArea ? `Search every Area for “${text}”` : `All results for “${text}”`}
        </a>
      {/if}
    </div>
  {/if}
</div>

<style>
  .search {
    position: relative;
  }

  .field {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--height-control);
    padding: 0 var(--space-2);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
    background: var(--color-surface);
    color: var(--color-muted);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) var(--ease-out);
  }

  .field:hover {
    border-color: var(--color-line-strong);
  }

  .field:focus-within {
    border-color: var(--color-focus);
    box-shadow: 0 0 0 var(--ring) var(--color-focus-soft);
  }

  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    outline: 0;
    background: none;
    box-shadow: none;
    color: var(--color-text);
    font-size: var(--text-m);
  }

  input:focus {
    box-shadow: none;
  }

  input::placeholder {
    color: var(--color-muted);
  }

  /* Escape clears the field, so the browser's own button would be a second way to say the same. */
  input::-webkit-search-cancel-button {
    display: none;
  }

  kbd {
    flex: none;
    padding: 0 var(--space-1);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-s);
    color: var(--color-muted);
    font-family: var(--font-body);
    font-size: var(--text-xs);
  }

  /* The list grows out of the field, wider than the sidebar, so it reads as the field's own. */
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
    border-radius: var(--radius-l);
    box-shadow: var(--shadow-float);
    transform-origin: top left;
    animation: results-in var(--dur-fast) var(--ease-out);
  }

  @keyframes results-in {
    from {
      opacity: 0;
      transform: scale(0.98) translateY(calc(-1 * var(--space-1)));
    }
  }

  .scope {
    display: flex;
    gap: var(--space-half);
    margin-bottom: var(--space-1);
    padding: var(--space-half);
    border-radius: var(--radius-m);
    background: var(--color-side);
  }

  .scope button {
    flex: 1;
    min-width: 0;
    height: var(--height-row);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: var(--color-muted);
    font-size: var(--text-s);
    font-weight: var(--weight-medium);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }

  .scope button:hover {
    color: var(--color-text);
  }

  .scope button[aria-pressed='true'] {
    background: var(--color-surface);
    color: var(--color-text);
    box-shadow: var(--shadow-float);
  }

  .scope-name {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .message {
    margin: 0;
    padding: var(--space-3);
    color: var(--color-muted);
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

  @media (max-width: 800px) {
    /* No keyboard to press it on. */
    kbd {
      display: none;
    }
  }
</style>
