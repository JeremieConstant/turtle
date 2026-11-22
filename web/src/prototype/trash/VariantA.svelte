<script lang="ts">
  /*
   * PROTOTYPE (issue #14), variant A: every Area has its own Trash.
   * The way in is a row at the end of the Area's tree in the sidebar. The
   * screen is that Area's tree of deleted Subtrees: each one unfolds to show
   * what went with it, and is restored as a whole.
   */
  import Icon from '../../components/Icon.svelte'
  import { navigation } from '../../lib/navigation.svelte'
  import { router } from '../../lib/router.svelte'
  import { areaAddress } from '../../lib/routes'
  import { SvelteSet } from 'svelte/reactivity'
  import { ago, beneath, daysLeft, inArea, KEEP_DAYS, leaves, purge, restore, type Node } from './state.svelte'

  const { areaId }: { areaId: string | null } = $props()

  const area = $derived(navigation.areas?.find(a => a.id === areaId) ?? navigation.areas?.[0] ?? null)
  const items = $derived(area ? inArea(area.id) : [])
  const open = new SvelteSet<string>()
  const toggle = (id: string) => (open.has(id) ? open.delete(id) : open.add(id))
</script>

{#snippet branch(nodes: Node[], depth: number)}
  <ul class="inner">
    {#each nodes as node}
      <li style:--depth={depth}>
        <span class="glyph"><Icon name={node.kind === 'folder' ? 'folder' : 'text'} size={14} /></span>
        <span class="title">{node.title}</span>
      </li>
      {#if node.children.length}{@render branch(node.children, depth + 1)}{/if}
    {/each}
  </ul>
{/snippet}

<main>
  {#if area}
    <a class="crumb" href={areaAddress(area.id, area.name)} onclick={event => router.follow(event, areaAddress(area.id, area.name))}>
      {area.name}
    </a>
    <h1>Trash</h1>
    <p class="sub">
      What is deleted from {area.name} stays here for {KEEP_DAYS} days. Restoring puts it back where it was, with everything beneath it.
    </p>

    {#if items.length}
      <ul class="entries">
        {#each items as item (item.id)}
          {@const count = beneath(item)}
          {@const isOpen = open.has(item.id)}
          <li class="entry">
            <div class="row">
              <button
                type="button"
                class="fold"
                class:open={isOpen}
                disabled={!count}
                aria-label={isOpen ? 'Hide what is beneath' : 'Show what is beneath'}
                aria-expanded={count ? isOpen : undefined}
                onclick={() => toggle(item.id)}
              >
                {#if count}<Icon name="chevron" size={14} />{/if}
              </button>
              <span class="glyph"><Icon name={item.kind === 'folder' ? 'folder' : 'text'} size={16} /></span>
              <span class="what">
                <span class="title">{item.title}</span>
                <span class="meta">
                  {#if item.parentGone}
                    Was in {item.was.join(' / ')}, which is gone. Restores to the top of {area.name}.
                  {:else if item.was.length}
                    Was in {item.was.join(' / ')}
                  {:else}
                    Was at the top of {area.name}
                  {/if}
                  · {item.by}, {ago(item.daysAgo)}{#if count}
                    · {count} beneath{/if}
                </span>
              </span>
              <span class="left" class:soon={daysLeft(item) <= 3}>{leaves(item)}</span>
              <button type="button" class="btn" onclick={() => restore([item.id])}>Restore</button>
              <button type="button" class="icon-btn" title="Delete for good" aria-label="Delete {item.title} for good" onclick={() => purge([item.id])}>
                <Icon name="close" size={14} />
              </button>
            </div>
            {#if isOpen}{@render branch(item.children, 1)}{/if}
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty"><p>The Trash of {area.name} is empty.</p></div>
    {/if}
  {/if}
</main>

<style>
  main {
    max-width: var(--width-text);
    margin: 0 auto;
    padding: var(--space-12) var(--space-6) var(--space-16);
  }

  .crumb {
    color: var(--color-muted);
    font-size: var(--text-m);
    text-decoration: none;
  }

  .crumb:hover {
    color: var(--color-text);
  }

  h1 {
    margin: var(--space-1) 0 0;
    font-size: var(--text-h1);
    font-weight: var(--weight-strong);
    letter-spacing: var(--tracking-display);
    line-height: var(--leading-tight);
  }

  .sub {
    margin: var(--space-2) 0 var(--space-8);
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .entries {
    margin: 0 calc(-1 * var(--space-3));
  }

  .entry {
    border-radius: var(--radius-m);
  }

  .entry:hover {
    background: var(--color-side);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--space-12);
    padding: var(--space-1) var(--space-2) var(--space-1) var(--space-1);
  }

  .fold {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--size-chevron);
    height: var(--size-chevron);
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: var(--color-muted);
  }

  .fold.open {
    transform: rotate(90deg);
  }

  .glyph {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--space-4);
    color: var(--color-muted);
  }

  .what {
    flex: 1;
    min-width: 0;
    display: grid;
    padding-left: var(--space-1);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-s);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .left {
    flex: none;
    color: var(--color-muted);
    font-size: var(--text-s);
    font-variant-numeric: tabular-nums;
  }

  .left.soon {
    color: var(--color-danger);
  }

  .inner li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--height-row);
    padding-left: calc(var(--space-8) + var(--depth) * var(--indent));
    padding-right: var(--space-3);
    color: var(--color-text-2);
    font-size: var(--text-m);
  }

  .entry > .inner {
    padding-bottom: var(--space-2);
  }

  .empty {
    padding: var(--space-8) 0;
    border-top: var(--line) solid var(--color-line);
    color: var(--color-text-2);
  }

  @media (max-width: 800px) {
    main {
      padding: var(--space-16) var(--space-4) var(--space-12);
    }

    .left {
      display: none;
    }
  }
</style>
