<script lang="ts">
  /* Search results: the title, where the Page sits, and the Snippet with the words that matched. */
  import { cubicOut } from 'svelte/easing'
  import { dur } from '../lib/motion'
  import type { SearchResult } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { place } from '../lib/search'

  const {
    results,
    compact = false,
    listbox,
    marked = -1,
    onpick,
  }: {
    results: SearchResult[]
    /** Closer together, for a list that floats under a search field. */
    compact?: boolean
    /**
     * The ID to give the list when a search field steers it with the arrow
     * keys. The results are then its options, each with the ID of the list
     * and its position, and `marked` is the one the field points at.
     */
    listbox?: string
    marked?: number
    /** Called once a result has been opened in this window. */
    onpick?: () => void
  } = $props()

  /** How much later each result arrives than the one above it, in milliseconds. */
  const STAGGER = 18

  // Results arrive one after the other, only the first few, never longer than a beat.
  function arrive(_node: HTMLElement, { index }: { index: number }) {
    const duration = dur('--dur-base')
    return {
      // Reduced motion leaves no duration, and then nothing waits either.
      delay: duration ? Math.min(index, 6) * STAGGER : 0,
      duration,
      easing: cubicOut,
      css: (t: number, u: number) => `opacity:${t};transform:translateY(calc(${u} * var(--space-1)))`,
    }
  }

  function open(event: MouseEvent, address: string) {
    router.follow(event, address)
    if (event.defaultPrevented) onpick?.()
  }
</script>

<ul class:compact id={listbox} role={listbox ? 'listbox' : undefined} aria-label="Search results">
  {#each results as result, index (result.page_id)}
    {@const address = pageAddress(result.page_id, result.title)}
    <li
      in:arrive={{ index }}
      id={listbox ? `${listbox}-${index}` : undefined}
      role={listbox ? 'option' : undefined}
      aria-selected={listbox ? index === marked : undefined}
    >
      <a class:marked={index === marked} href={address} onclick={event => open(event, address)}>
        <span class="title">{result.title}</span>
        <span class="place">{place(result)}</span>
        {#if result.snippet.length > 0}
          <span class="snippet"
            >{#each result.snippet as piece, at (at)}{#if piece.highlighted}<mark>{piece.text}</mark
                >{:else}{piece.text}{/if}{/each}</span
          >
        {/if}
      </a>
    </li>
  {/each}
</ul>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  a {
    display: grid;
    gap: var(--space-half);
    padding: var(--space-3);
    border-radius: var(--radius-m);
    text-decoration: none;
    transition: background var(--dur-fast) var(--ease-out);
  }

  .compact a {
    padding: var(--space-2) var(--space-3);
  }

  a:hover,
  a:focus-visible,
  a.marked {
    background: var(--color-hover);
  }

  .title {
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
    color: var(--color-text);
  }

  .compact .title {
    font-size: var(--text-m);
  }

  .place {
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-s);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .snippet {
    margin-top: var(--space-1);
    color: var(--color-text-2);
    font-size: var(--text-m);
    line-height: var(--leading-ui);
  }

  .compact .snippet {
    margin-top: 0;
    font-size: var(--text-s);
  }

  /* The palette is monochrome, so a match is told by weight and a line, not by a colour. */
  mark {
    background: none;
    color: var(--color-text);
    font-weight: var(--weight-strong);
    text-decoration: underline;
    text-decoration-color: var(--color-line-strong);
    text-underline-offset: var(--space-half);
  }
</style>
