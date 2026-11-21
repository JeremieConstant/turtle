<script lang="ts">
  /*
   * The start screen answers two things: where is what I need (search), and
   * can I write it down right now (a Quick Note). Nothing else competes.
   */
  import { cubicOut } from 'svelte/easing'
  import { flip } from 'svelte/animate'
  import Hits from '../components/Hits.svelte'
  import Icon from '../components/Icon.svelte'
  import { dur } from '../lib/motion'
  import { create_quick_note, list_areas, list_quick_notes, move_entry, search } from '../lib/mock/ops.svelte'
  import { router } from '../lib/router.svelte'
  import { pageAddress, searchAddress } from '../lib/routes'
  import { relativeTime } from '../lib/time'

  let query = $state('')
  let active = $state(0)
  let searchFocused = $state(false)
  const hits = $derived(query.trim() ? search({ query, limit: 6 }) : [])

  let note = $state('')
  let justSaved = $state<string | null>(null)
  let savedTimer: ReturnType<typeof setTimeout> | undefined
  const notes = $derived(list_quick_notes({ limit: 5 }))
  const areas = $derived(list_areas())
  const areaName = (id: string) => areas.find((a) => a.id === id)?.name ?? ''
  const targets = $derived(areas.filter((a) => !a.personal && a.role !== 'read'))

  let searchInput: HTMLInputElement
  let noteInput: HTMLTextAreaElement
  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 15_000)
    return () => clearInterval(t)
  })

  function onSearchKey(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' && hits.length) (active = (active + 1) % hits.length), event.preventDefault()
    else if (event.key === 'ArrowUp' && hits.length) (active = (active - 1 + hits.length) % hits.length), event.preventDefault()
    else if (event.key === 'Escape') query = ''
    else if (event.key === 'Enter') {
      event.preventDefault()
      const hit = hits[active]
      if (hit && hit.kind === 'page' && !event.shiftKey) router.navigate(pageAddress(hit.id, hit.title))
      else if (query.trim()) router.navigate(searchAddress(query.trim()))
    }
  }

  function save() {
    if (!note.trim()) return
    const id = create_quick_note({ text: note })
    note = ''
    justSaved = id
    clearTimeout(savedTimer)
    savedTimer = setTimeout(() => (justSaved = null), 2200)
    noteInput.style.height = ''
    noteInput.focus()
  }

  function onNoteKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault()
      save()
    }
  }

  function moveTo(noteId: string, areaId: string) {
    move_entry({ entry_id: noteId, area_id: areaId, parent_id: null, index: 0 })
    ;(document.getElementById(`move-${noteId}`) as HTMLElement & { hidePopover(): void }).hidePopover()
  }

  function grow(event: Event) {
    const el = event.currentTarget as HTMLTextAreaElement
    el.style.height = 'auto'
    el.style.height = Math.min(el.scrollHeight, 320) + 'px'
  }

  /* The note that was just saved drops into the list from the box above. */
  function land(_node: HTMLElement) {
    return {
      duration: dur('--dur-slow'),
      easing: cubicOut,
      css: (t: number, u: number) => `opacity:${t};transform:translateY(${-u * 10}px)`,
    }
  }

  // "/" anywhere on the screen jumps to the search, like most developer tools.
  function onWindowKey(event: KeyboardEvent) {
    const typing = (event.target as Element).closest('input, textarea, [contenteditable]')
    if (event.key === '/' && !typing) {
      event.preventDefault()
      searchInput.focus()
    }
  }
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="start">
  <div class="column">
    <div class="search-wrap">
      <form class="search" role="search" onsubmit={(e) => e.preventDefault()}>
        <Icon name="search" size={20} />
        <!-- svelte-ignore a11y_autofocus -->
        <input
          bind:this={searchInput}
          bind:value={query}
          type="search"
          placeholder="Search every Area"
          aria-label="Search every Area"
          autocomplete="off"
          autofocus
          oninput={() => (active = 0)}
          onkeydown={onSearchKey}
          onfocus={() => (searchFocused = true)}
          onblur={() => setTimeout(() => (searchFocused = false), 150)}
        />
        <kbd aria-hidden="true">/</kbd>
      </form>
      {#if hits.length && searchFocused}
        <div class="results">
          <Hits {hits} {active} compact />
          <a class="all" href={searchAddress(query.trim())} onclick={(e) => router.follow(e, searchAddress(query.trim()))}>
            All results for “{query.trim()}”
          </a>
        </div>
      {:else if query.trim() && searchFocused}
        <div class="results"><p class="none">Nothing matches “{query.trim()}”</p></div>
      {/if}
    </div>

    <section class="quick" aria-labelledby="quick-label">
      <h2 id="quick-label" class="visually-hidden">Quick Note</h2>
      <div class="box" class:has-text={!!note.trim()}>
        <textarea
          bind:this={noteInput}
          bind:value={note}
          rows="3"
          placeholder="Write a Quick Note"
          aria-label="Quick Note"
          oninput={grow}
          onkeydown={onNoteKey}
        ></textarea>
        <div class="box-foot">
          <span class="hint">
            {#if justSaved}
              <span class="saved">Saved to Personal</span>
            {:else}
              Lands in Personal
            {/if}
          </span>
          <button class="btn primary" disabled={!note.trim()} onclick={save}>
            Save <kbd class="on-dark">⌘↵</kbd>
          </button>
        </div>
      </div>

      {#if notes.length}
        <ul class="notes" aria-label="Recent Quick Notes">
          {#each notes as n (n.id)}
            <li class="note" class:fresh={n.id === justSaved} animate:flip={{ duration: dur('--dur-slow'), easing: cubicOut }} in:land>
              <a class="note-link" href={pageAddress(n.id, n.title)} onclick={(e) => router.follow(e, pageAddress(n.id, n.title))}>
                <span class="note-title">{n.title}</span>
                {#if n.text}<span class="note-text">{n.text}</span>{/if}
              </a>
              <span class="note-side">
                {#if n.area_id !== 'personal'}
                  <span class="moved">in {areaName(n.area_id)}</span>
                {/if}
                <time>{relativeTime(n.created_at, now)}</time>
                {#if n.area_id === 'personal'}
                  <button class="move icon-btn" title="Move to an Area" aria-label="Move “{n.title}” to an Area" popovertarget="move-{n.id}" style:anchor-name="--move-{n.id}">
                    <Icon name="move" size={14} />
                  </button>
                  <div class="menu" id="move-{n.id}" popover style:position-anchor="--move-{n.id}">
                    <p class="menu-label">Move to</p>
                    {#each targets as area (area.id)}
                      <button onclick={() => moveTo(n.id, area.id)}>{area.name}</button>
                    {/each}
                  </div>
                {/if}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  </div>
</div>

<style>
  .start {
    min-height: 100%;
    padding: max(var(--space-16), 14vh) var(--space-6) var(--space-16);
  }

  .column {
    max-width: var(--width-start);
    margin: 0 auto;
  }

  .search-wrap {
    position: relative;
    margin-bottom: var(--space-12);
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 56px;
    padding: 0 var(--space-4);
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-l);
    background: var(--color-surface);
    color: var(--color-muted);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) var(--ease-out);
  }

  .search:focus-within {
    border-color: var(--color-focus);
    box-shadow: 0 0 0 4px var(--color-focus-soft);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--color-text);
    font-size: var(--text-search);
    letter-spacing: var(--tracking-heading);
  }

  .search input::placeholder {
    color: var(--color-faint);
  }

  .search input::-webkit-search-cancel-button {
    display: none;
  }

  kbd {
    padding: 1px 6px;
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-s);
    font-family: var(--font-body);
    font-size: var(--text-xs);
    color: var(--color-faint);
  }

  kbd.on-dark {
    border-color: rgb(255 255 255 / 0.25);
    color: rgb(255 255 255 / 0.7);
    padding: 0 4px;
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
    transform-origin: top center;
  }

  @keyframes results-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
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

  .all:hover {
    color: var(--color-text);
  }

  .none {
    margin: 0;
    padding: var(--space-3);
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  .box {
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-l);
    background: var(--color-side);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) var(--ease-out);
  }

  .box:focus-within {
    border-color: var(--color-focus);
    background: var(--color-surface);
    box-shadow: 0 0 0 4px var(--color-focus-soft);
  }

  .box textarea {
    display: block;
    width: 100%;
    min-height: 88px;
    padding: var(--space-4) var(--space-4) var(--space-2);
    border: 0;
    outline: 0;
    resize: none;
    background: none;
    font-size: var(--text-base);
    line-height: var(--leading-body);
  }

  .box textarea::placeholder {
    color: var(--color-faint);
  }

  .box-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--space-3) var(--space-3) var(--space-4);
  }

  .hint {
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .saved {
    color: var(--color-text);
    animation: saved-in var(--dur-base) var(--ease-out);
  }

  @keyframes saved-in {
    from {
      opacity: 0;
      transform: translateY(3px);
    }
  }

  .notes {
    margin: var(--space-6) 0 0;
    padding: 0;
    list-style: none;
  }

  .note {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin: 0 calc(-1 * var(--space-3));
    padding: 0 var(--space-3);
    border-radius: var(--radius-m);
    transition: background var(--dur-fast) var(--ease-out);
  }

  .note + .note::before {
    content: '';
    position: absolute;
    top: 0;
    left: var(--space-3);
    right: var(--space-3);
    border-top: var(--line) solid var(--color-line);
  }

  .note:hover {
    background: var(--color-side);
  }

  .note:hover::before,
  .note:hover + .note::before {
    border-color: transparent;
  }

  .note.fresh {
    animation: fresh var(--dur-agent) var(--ease-out);
  }

  @keyframes fresh {
    from {
      background: var(--color-focus-soft);
    }
  }

  .note-link {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: var(--space-half);
    padding: var(--space-3) 0;
    text-decoration: none;
  }

  .note-title {
    font-size: var(--text-m);
    font-weight: var(--weight-medium);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note-text {
    color: var(--color-muted);
    font-size: var(--text-s);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note-side {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-s);
    white-space: nowrap;
  }

  .moved {
    color: var(--color-text-2);
  }

  .move {
    opacity: 0;
    transition: opacity var(--dur-fast) var(--ease-out);
  }

  .note:hover .move,
  .move:focus-visible,
  .move:has(+ :popover-open) {
    opacity: 1;
  }

  .menu {
    position: fixed;
    position-area: bottom span-left;
    margin-top: var(--space-1);
    min-width: 180px;
  }

  .menu-label {
    margin: 0;
    padding: var(--space-2) var(--space-3) var(--space-1);
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .menu button {
    display: block;
    width: 100%;
    height: 30px;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    font-size: var(--text-m);
    text-align: left;
  }

  .menu button:hover,
  .menu button:focus-visible {
    background: var(--color-hover);
    outline: none;
  }

  @media (hover: none) {
    .move {
      opacity: 1;
    }
  }

  @media (max-width: 800px) {
    .start {
      padding: var(--space-16) var(--space-4) var(--space-12);
    }

    .search input {
      font-size: var(--text-l);
    }

    kbd {
      display: none;
    }
  }
</style>
