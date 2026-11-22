<script lang="ts">
  /*
   * PROTOTYPE (issue #14), variant B: one Trash for the whole Workspace.
   * The way in is one row at the foot of the sidebar. The screen is a table
   * of everything the person may restore, ordered by how soon it is gone,
   * narrowed by Area, with rows ticked and restored together.
   */
  import { SvelteSet } from 'svelte/reactivity'
  import Icon from '../../components/Icon.svelte'
  import { navigation } from '../../lib/navigation.svelte'
  import { ago, areaOf, beneath, daysLeft, KEEP_DAYS, purge, restore, trash, type Trashed } from './state.svelte'

  let only = $state<string | null>(null)
  const picked = new SvelteSet<string>()

  const areas = $derived((navigation.areas ?? []).filter(area => trash.items.some(item => areaOf(item)?.id === area.id)))
  const shown = $derived(trash.items.filter(item => !only || areaOf(item)?.id === only))

  const GROUPS: { title: string; has: (item: Trashed) => boolean }[] = [
    { title: 'Gone within a week', has: item => daysLeft(item) <= 7 },
    { title: 'Deleted this week', has: item => daysLeft(item) > 7 && item.daysAgo <= 7 },
    { title: 'Earlier', has: item => daysLeft(item) > 7 && item.daysAgo > 7 },
  ]
  const groups = $derived(
    GROUPS.map(group => ({ title: group.title, items: shown.filter(group.has).sort((a, b) => b.daysAgo - a.daysAgo) })).filter(
      group => group.items.length,
    ),
  )

  const chosen = $derived(shown.filter(item => picked.has(item.id)).map(item => item.id))
  const tick = (id: string) => (picked.has(id) ? picked.delete(id) : picked.add(id))

  function act(what: (ids: string[]) => void) {
    what(chosen)
    picked.clear()
  }
</script>

<main>
  <h1>Trash</h1>
  <p class="sub">Everything deleted in the Areas you can write in. After {KEEP_DAYS} days it is gone for good.</p>

  <div class="filter" role="group" aria-label="Area">
    <button type="button" class="chip" aria-pressed={only === null} onclick={() => (only = null)}>All Areas</button>
    {#each areas as area (area.id)}
      <button type="button" class="chip" aria-pressed={only === area.id} onclick={() => (only = area.id)}>{area.name}</button>
    {/each}
  </div>

  {#if groups.length}
    <table>
      <thead>
        <tr>
          <th class="tick"><span class="visually-hidden">Pick</span></th>
          <th>Title</th>
          <th>Area</th>
          <th>Deleted</th>
          <th class="num">Left</th>
        </tr>
      </thead>
      {#each groups as group (group.title)}
        <tbody>
          <tr class="group"><th colspan="5">{group.title}</th></tr>
          {#each group.items as item (item.id)}
            {@const count = beneath(item)}
            <tr class:picked={picked.has(item.id)}>
              <td class="tick">
                <input type="checkbox" aria-label="Pick {item.title}" checked={picked.has(item.id)} onchange={() => tick(item.id)} />
              </td>
              <td>
                <span class="name">
                  <span class="glyph"><Icon name={item.kind === 'folder' ? 'folder' : 'text'} size={14} /></span>
                  <span class="title">{item.title}</span>
                  {#if count}<span class="count">+{count}</span>{/if}
                </span>
              </td>
              <td class="muted-cell">{areaOf(item)?.name}{#if item.was.length}<span class="path"> / {item.was.join(' / ')}</span>{/if}</td>
              <td class="muted-cell">{ago(item.daysAgo)} · {item.by}</td>
              <td class="num" class:soon={daysLeft(item) <= 3}>{daysLeft(item)}</td>
            </tr>
          {/each}
        </tbody>
      {/each}
    </table>
  {:else}
    <p class="empty">Nothing in the Trash.</p>
  {/if}
</main>

{#if chosen.length}
  <div class="actions" role="region" aria-label="Picked Entries">
    <span>{chosen.length} picked</span>
    <button type="button" class="btn primary" onclick={() => act(restore)}>Restore</button>
    <button type="button" class="btn danger" onclick={() => act(purge)}>Delete for good</button>
    <button type="button" class="btn" onclick={() => picked.clear()}>Cancel</button>
  </div>
{/if}

<style>
  main {
    max-width: var(--width-content);
    margin: 0 auto;
    padding: var(--space-12) var(--space-8) var(--space-16);
  }

  h1 {
    margin: 0;
    font-size: var(--text-h1);
    font-weight: var(--weight-strong);
    letter-spacing: var(--tracking-display);
    line-height: var(--leading-tight);
  }

  .sub {
    margin: var(--space-2) 0 var(--space-6);
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  .filter {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-bottom: var(--space-6);
  }

  .chip {
    height: var(--height-control);
    padding: 0 var(--space-3);
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-round);
    background: none;
    color: var(--color-text-2);
    font-size: var(--text-s);
    font-weight: var(--weight-normal);
  }

  .chip[aria-pressed='true'] {
    border-color: var(--color-text);
    background: var(--color-text);
    color: var(--color-bg);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-m);
    table-layout: fixed;
  }

  th,
  td {
    height: var(--height-row-l);
    padding: 0 var(--space-3);
    border-bottom: var(--line) solid var(--color-line);
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  thead th {
    height: var(--height-row);
    color: var(--color-muted);
    font-size: var(--text-s);
    font-weight: var(--weight-medium);
  }

  .group th {
    height: var(--space-10);
    padding-top: var(--space-4);
    border-bottom-color: var(--color-line-strong);
    color: var(--color-text);
    font-size: var(--text-s);
    font-weight: var(--weight-strong);
    vertical-align: bottom;
  }

  .tick {
    width: var(--space-10);
    padding-right: 0;
    text-overflow: clip;
  }

  th:nth-child(3) {
    width: 20%;
  }

  th:nth-child(4) {
    width: 28%;
  }

  .num {
    width: var(--space-16);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  td.num {
    width: auto;
  }

  .soon {
    color: var(--color-danger);
    font-weight: var(--weight-strong);
  }

  tbody tr:not(.group):hover,
  tr.picked {
    background: var(--color-side);
  }

  .name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .glyph,
  .count,
  .muted-cell,
  .path {
    color: var(--color-muted);
  }

  .glyph {
    flex: none;
  }

  .count {
    flex: none;
    font-size: var(--text-s);
  }

  .path {
    color: var(--color-faint);
  }

  .empty {
    color: var(--color-text-2);
  }

  /* The bar for the ticked rows sits above the prototype bar. */
  .actions {
    position: fixed;
    left: 50%;
    bottom: var(--space-16);
    z-index: var(--layer-menu);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-l);
    background: var(--color-surface);
    font-size: var(--text-m);
    box-shadow: var(--shadow-float);
    transform: translateX(-50%);
    white-space: nowrap;
  }

  @media (max-width: 800px) {
    main {
      padding: var(--space-16) var(--space-4) var(--space-12);
    }

    th:nth-child(3),
    td:nth-child(3),
    th:nth-child(4),
    td:nth-child(4) {
      display: none;
    }
  }
</style>
