<script lang="ts">
  /*
   * PROTOTYPE (issue #14), variant C: there is no Trash screen.
   * A deleted Entry stays where it was in the tree of its Area, out of sight
   * until "Show deleted" is switched on, and is restored in place. The way in
   * is that switch on the Area screen, so the sidebar gets nothing new.
   */
  import Icon from '../../components/Icon.svelte'
  import { navigation } from '../../lib/navigation.svelte'
  import type { Entry } from '../../lib/operations.gen'
  import { ago, beneath, daysLeft, inArea, KEEP_DAYS, leaves, purge, restore, type Node, type Trashed } from './state.svelte'

  const { areaId }: { areaId: string | null } = $props()

  const area = $derived(navigation.areas?.find(a => a.id === areaId) ?? navigation.areas?.[0] ?? null)
  $effect(() => {
    if (area && !navigation.tree(area.id)) void navigation.loadTree(area.id).catch(() => {})
  })
  const tree = $derived(area ? (navigation.tree(area.id) ?? []) : [])
  const deleted = $derived(area ? inArea(area.id) : [])
  let showDeleted = $state(true)

  /* The made-up deleted Entries have no real place, so they are dealt out:
     those from the top stay at the top, the others go beneath the live
     Entries that can hold something, one after the other. */
  const holders = $derived(tree.filter(entry => entry.kind === 'folder' || entry.children.length))
  const placed = $derived.by(() => {
    const at = new Map<string | null, Trashed[]>()
    let next = 0
    for (const item of deleted) {
      const home = item.was.length && !item.parentGone && holders.length ? holders[next++ % holders.length].id : null
      at.set(home, [...(at.get(home) ?? []), item])
    }
    return at
  })
</script>

{#snippet gone(nodes: Node[], depth: number)}
  {#each nodes as node}
    <li class="deleted child" style:--depth={depth}>
      <span class="glyph"><Icon name={node.kind === 'folder' ? 'folder' : 'text'} size={14} /></span>
      <span class="title">{node.title}</span>
    </li>
    {@render gone(node.children, depth + 1)}
  {/each}
{/snippet}

{#snippet trashed(items: Trashed[], depth: number)}
  {#each items as item (item.id)}
    <li class="deleted" style:--depth={depth}>
      <span class="glyph"><Icon name={item.kind === 'folder' ? 'folder' : 'text'} size={16} /></span>
      <span class="title">{item.title}</span>
      <span class="meta" class:soon={daysLeft(item) <= 3}>
        Deleted {ago(item.daysAgo)} by {item.by} · {leaves(item).toLowerCase()}
      </span>
      <button type="button" class="btn" onclick={() => restore([item.id])}>Restore{#if beneath(item)}&nbsp;all {beneath(item) + 1}{/if}</button>
      <button type="button" class="icon-btn" title="Delete for good" aria-label="Delete {item.title} for good" onclick={() => purge([item.id])}>
        <Icon name="close" size={14} />
      </button>
    </li>
    {@render gone(item.children, depth + 1)}
  {/each}
{/snippet}

{#snippet live(entries: Entry[], depth: number)}
  {#each entries as entry (entry.id)}
    <li style:--depth={depth}>
      <span class="glyph"><Icon name={entry.kind === 'folder' ? 'folder' : 'text'} size={16} /></span>
      <span class="title">{entry.title}</span>
    </li>
    {@render live(entry.children, depth + 1)}
    {#if showDeleted}{@render trashed(placed.get(entry.id) ?? [], depth + 1)}{/if}
  {/each}
{/snippet}

<main>
  {#if area}
    <div class="head">
      <h1>{area.name}</h1>
      <label class="switch">
        <input type="checkbox" bind:checked={showDeleted} />
        Show deleted ({deleted.length})
      </label>
    </div>
    <p class="sub">
      {#if showDeleted}
        Deleted Pages and Folders are shown where they were. They are gone for good {KEEP_DAYS} days after deleting.
      {:else}
        You can write here.
      {/if}
    </p>

    <ul class="entries">
      {@render live(tree, 0)}
      {#if showDeleted}{@render trashed(placed.get(null) ?? [], 0)}{/if}
    </ul>
    {#if !tree.length && !(showDeleted && deleted.length)}
      <p class="sub">Nothing here yet.</p>
    {/if}
  {/if}
</main>

<style>
  main {
    max-width: var(--width-text);
    margin: 0 auto;
    padding: var(--space-12) var(--space-6) var(--space-16);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  h1 {
    margin: 0;
    font-size: var(--text-h1);
    font-weight: var(--weight-strong);
    letter-spacing: var(--tracking-display);
    line-height: var(--leading-tight);
  }

  .switch {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-2);
    font-size: var(--text-m);
    white-space: nowrap;
  }

  .sub {
    margin: var(--space-2) 0 var(--space-8);
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  .entries {
    margin: 0 calc(-1 * var(--space-3)) var(--space-8);
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--height-row-l);
    padding-left: calc(var(--space-3) + var(--depth) * var(--space-6));
    padding-right: var(--space-2);
    border-radius: var(--radius-m);
  }

  li:hover {
    background: var(--color-side);
  }

  .glyph {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--space-4);
    color: var(--color-muted);
  }

  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  li:not(.deleted) .title {
    flex: 1;
  }

  /* A deleted Entry reads as absent: fainter, struck through, on a sunken band. */
  .deleted {
    background: var(--color-sunken);
    border-radius: 0;
  }

  .deleted .title {
    color: var(--color-muted);
    text-decoration: line-through;
  }

  .deleted.child {
    height: var(--height-row);
    font-size: var(--text-m);
  }

  .deleted.child .title {
    flex: 1;
  }

  .meta {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-s);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta.soon {
    color: var(--color-danger);
  }

  @media (max-width: 800px) {
    main {
      padding: var(--space-16) var(--space-4) var(--space-12);
    }

    .meta {
      display: none;
    }

    .deleted .title {
      flex: 1;
    }
  }
</style>
