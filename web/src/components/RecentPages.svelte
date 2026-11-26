<script lang="ts">
  /* The Pages edited last, by anyone, among those the person can read: where the work is going on. */
  import { call } from '../lib/ops'
  import type { RecentPage } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { relativeTime } from '../lib/time'
  import { reportFailure } from '../lib/toasts'
  import AreaMark from './AreaMark.svelte'

  /** How many are listed. */
  const LISTED = 6

  let pages = $state.raw<RecentPage[] | null>(null)
  let now = $state(Math.floor(Date.now() / 1000))

  $effect(() => {
    void load()
    const ticking = setInterval(() => (now = Math.floor(Date.now() / 1000)), 30_000)
    return () => clearInterval(ticking)
  })

  /** Loads the list again, as after a Quick Note was saved. */
  export async function load() {
    try {
      pages = (await call('list_recent_pages', { limit: LISTED })).pages
    } catch (caught) {
      reportFailure(caught, 'The recently edited Pages could not be loaded. Reload the page to try again.')
    }
  }
</script>

{#if pages?.length}
  <section class="section" aria-labelledby="recent-title">
    <h2 class="section-title" id="recent-title">Recently edited</h2>
    <ul>
      {#each pages as page (page.id)}
        {@const address = pageAddress(page.id, page.title)}
        <li>
          <a href={address} onclick={event => router.follow(event, address)}>
            <AreaMark area={page.area} />
            <span class="title">{page.title}</span>
            <span class="side">
              <span class="area">{page.area.name}</span>
              <time datetime={new Date(page.last_edited.at * 1000).toISOString()}>{relativeTime(page.last_edited.at, now)}</time>
            </span>
          </a>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .section {
    margin-top: var(--space-12);
  }

  ul {
    margin: var(--space-3) calc(-1 * var(--space-3)) 0;
    padding: 0;
    list-style: none;
  }

  a {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--height-row-l);
    padding: 0 var(--space-3);
    border-radius: var(--radius-m);
    color: var(--color-text);
    text-decoration: none;
    transition: background var(--dur-fast) var(--ease-out);
  }

  a:hover {
    background: var(--color-side);
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-m);
    font-weight: var(--weight-medium);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .side {
    display: flex;
    gap: var(--space-3);
    color: var(--color-muted);
    font-size: var(--text-s);
    white-space: nowrap;
  }

  @media (max-width: 800px) {
    .area {
      display: none;
    }
  }
</style>
