<script lang="ts">
  /* Search results: title, where it sits, and the Snippet with its matches. */
  import { dur } from '../lib/motion'
  import type { SearchHit } from '../lib/mock/types'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { cubicOut } from 'svelte/easing'
  import Icon from './Icon.svelte'

  let { hits, active = -1, compact = false, onpick }: { hits: SearchHit[]; active?: number; compact?: boolean; onpick?: () => void } =
    $props()

  // Results arrive with a short stagger, only the first few, never longer than a beat.
  function arrive(_node: HTMLElement, { i }: { i: number }) {
    return {
      delay: Math.min(i, 6) * 18,
      duration: dur('--dur-base'),
      easing: cubicOut,
      css: (t: number, u: number) => `opacity:${t};transform:translateY(${u * 4}px)`,
    }
  }
</script>

<ul class="hits" class:compact role="listbox" aria-label="Search results">
  {#each hits as hit, i (hit.id)}
    {@const href = hit.kind === 'page' ? pageAddress(hit.id, hit.title) : null}
    <li in:arrive={{ i }} role="option" aria-selected={i === active}>
      <a
        class="hit"
        class:active={i === active}
        href={href ?? undefined}
        onclick={(e) => href && (router.follow(e, href), onpick?.())}
      >
        <span class="title">
          {#if hit.kind === 'folder'}<Icon name="folder" size={14} />{/if}
          {hit.title}
        </span>
        <span class="where">{[hit.area.name, ...hit.path].join(' / ')}</span>
        {#if !compact && hit.snippet.length}
          <span class="snippet">
            {#each hit.snippet as f, j (j)}{#if f.match}<mark>{f.text}</mark>{:else}{f.text}{/if}{/each}
          </span>
        {/if}
      </a>
    </li>
  {/each}
</ul>

<style>
  .hits {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .hit {
    display: grid;
    gap: var(--space-half);
    padding: var(--space-3) var(--space-3);
    border-radius: var(--radius-m);
    text-decoration: none;
    transition: background var(--dur-fast) var(--ease-out);
  }

  .compact .hit {
    padding: var(--space-2) var(--space-3);
  }

  .hit:hover,
  .hit.active {
    background: var(--color-hover);
  }

  .title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
    color: var(--color-text);
  }

  .compact .title {
    font-size: var(--text-m);
  }

  .title :global(svg) {
    color: var(--color-muted);
  }

  .where {
    color: var(--color-muted);
    font-size: var(--text-s);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .snippet {
    margin-top: var(--space-1);
    color: var(--color-text-2);
    font-size: var(--text-m);
    line-height: var(--leading-ui);
  }

  mark {
    background: none;
    color: var(--color-text);
    font-weight: var(--weight-strong);
    text-decoration: underline;
    text-decoration-color: var(--color-line-strong);
    text-underline-offset: 2px;
  }
</style>
