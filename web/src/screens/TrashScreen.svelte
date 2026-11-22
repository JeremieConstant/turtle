<script lang="ts">
  /*
   * The Trash of one Area: each deleted Page or Folder once, as the Subtree
   * that was deleted, newest first. A row unfolds to show what went with it;
   * it is restored, or deleted for good, only as a whole.
   */
  import { untrack } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { toast } from 'svelte-sonner'
  import Icon from '../components/Icon.svelte'
  import { ask } from '../lib/confirm.svelte'
  import { navigation } from '../lib/navigation.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Area, Entry, TrashEntry } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { areaAddress, pageAddress, trashAddress } from '../lib/routes'
  import { exactTime, relativeTime } from '../lib/time'
  import { reportFailure } from '../lib/toasts'
  import { daysLeft, deletedBy, endsSoon, whereItWas } from '../lib/trash'
  import { ui } from '../lib/ui.svelte'
  import NotFoundScreen from './NotFoundScreen.svelte'

  // The screen is created anew for every Area, so the ID never changes underneath it.
  const props: { areaId: string } = $props()
  const areaId = untrack(() => props.areaId)

  let area = $state.raw<Area | null>(null)
  let entries = $state.raw<TrashEntry[]>([])
  /** Set when the Area does not exist or the caller may not read it; the two look the same. */
  let missing = $state(false)
  let refusal = $state<string | null>(null)
  let loadFailed = $state(false)
  let busy = $state(false)
  let now = $state(Date.now() / 1000)
  const unfolded = new SvelteSet<string>()

  // The sidebar shows the tree of the Area whose Trash this is.
  ui.openArea = areaId

  async function load() {
    try {
      const trash = await call('get_trash', { area_id: areaId })
      area = trash.area
      entries = trash.entries
      now = Date.now() / 1000
    } catch (caught) {
      if (caught instanceof OperationError && caught.kind === 'not_found') missing = true
      else if (caught instanceof OperationError && caught.kind === 'not_permitted') refusal = caught.message
      else loadFailed = true
    }
  }
  void load()

  $effect(() => {
    if (area) document.title = `Trash of ${area.name} · Turtle`
  })

  $effect(() => {
    if (!area) return
    const address = trashAddress(area.id, area.name)
    if (location.pathname !== address) router.replace(address)
  })

  /** Shows the Trash and the tree as they are after a change. */
  async function changed() {
    await load()
    try {
      await navigation.loadTree(areaId)
    } catch {
      // The sidebar says so itself when it cannot show the tree.
    }
  }

  async function restore(entry: TrashEntry) {
    busy = true
    try {
      const restored = await call('restore_from_trash', { id: entry.id })
      await changed()
      const where = restored.to_top ? `to the top of ${area?.name}` : 'to where it was'
      const address = restored.kind === 'page' ? pageAddress(restored.id, restored.title) : null
      toast(`Restored ${restored.title} ${where}`, address ? { action: { label: 'Open', onClick: () => router.navigate(address) } } : undefined)
    } catch (caught) {
      reportFailure(caught, `${entry.title} could not be restored. Try again.`)
    } finally {
      busy = false
    }
  }

  async function deleteForGood(entry: TrashEntry) {
    const beneath = entry.entries_beneath
    const yes = await ask({
      title: `Delete ${entry.title} for good?`,
      message:
        beneath > 0
          ? `${entry.title} and the ${beneath} ${beneath === 1 ? 'Page or Folder' : 'Pages and Folders'} beneath it are gone for good, with their Versions. This cannot be undone.`
          : `${entry.title} is gone for good, with its Versions. This cannot be undone.`,
      action: 'Delete for good',
      destructive: true,
    })
    if (!yes) return
    busy = true
    try {
      await call('delete_from_trash', { id: entry.id })
      await changed()
    } catch (caught) {
      reportFailure(caught, `${entry.title} could not be deleted. Try again.`)
    } finally {
      busy = false
    }
  }

  const toggle = (id: string) => (unfolded.has(id) ? unfolded.delete(id) : unfolded.add(id))
</script>

{#snippet branch(children: Entry[], depth: number)}
  <ul class="inner">
    {#each children as child (child.id)}
      <li style:--depth={depth}>
        <span class="glyph"><Icon name={child.kind === 'folder' ? 'folder' : 'text'} size={14} /></span>
        <span class="title">{child.title}</span>
      </li>
      {#if child.children.length > 0}{@render branch(child.children, depth + 1)}{/if}
    {/each}
  </ul>
{/snippet}

{#if missing}
  <NotFoundScreen />
{:else}
  <main>
    {#if refusal}
      <p class="sub">{refusal}</p>
    {:else if loadFailed}
      <p class="sub">This Trash could not be loaded. Reload the page to try again.</p>
    {:else if area}
      {@const toArea = areaAddress(area.id, area.name)}
      <a class="crumb" href={toArea} onclick={event => router.follow(event, toArea)}>{area.name}</a>
      <h1>Trash</h1>
      <p class="sub">
        What is deleted from {area.name} stays here for 30 days. Restoring puts it back where it was, with everything beneath it.
      </p>

      {#if entries.length > 0}
        <ul class="entries" aria-label="Deleted from {area.name}">
          {#each entries as entry (entry.id)}
            {@const open = unfolded.has(entry.id)}
            {@const count = entry.entries_beneath}
            <li class="entry">
              <div class="row">
                <button
                  type="button"
                  class="fold"
                  class:open
                  disabled={count === 0}
                  aria-label={open ? `Hide what was deleted with ${entry.title}` : `Show what was deleted with ${entry.title}`}
                  aria-expanded={count > 0 ? open : undefined}
                  onclick={() => toggle(entry.id)}
                >
                  {#if count > 0}<Icon name="chevron" size={14} />{/if}
                </button>
                <span class="glyph"><Icon name={entry.kind === 'folder' ? 'folder' : 'text'} size={16} /></span>
                <span class="what">
                  <span class="title">{entry.title}</span>
                  <span class="meta">
                    {whereItWas(entry, area.name)}
                    · {deletedBy(entry.deleted_by)},
                    <time datetime={new Date(entry.deleted_at * 1000).toISOString()} title={exactTime(entry.deleted_at)}>{relativeTime(entry.deleted_at, now)}</time>
                    {#if count > 0}· {count} beneath{/if}
                  </span>
                </span>
                <span class="left" class:soon={endsSoon(entry.days_left)}>{daysLeft(entry.days_left)}</span>
                <button type="button" class="btn" disabled={busy} onclick={() => restore(entry)}>Restore</button>
                <button
                  type="button"
                  class="icon-btn"
                  title="Delete for good"
                  aria-label="Delete {entry.title} for good"
                  disabled={busy}
                  onclick={() => deleteForGood(entry)}
                >
                  <Icon name="close" size={14} />
                </button>
              </div>
              {#if open}{@render branch(entry.children, 1)}{/if}
            </li>
          {/each}
        </ul>
      {:else}
        <div class="empty"><p>The Trash of {area.name} is empty.</p></div>
      {/if}
    {/if}
  </main>
{/if}

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
    transition: transform var(--dur-fast) var(--ease-out);
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
