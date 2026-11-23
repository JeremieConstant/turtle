<script lang="ts">
  /*
   * The tree of one Area in the sidebar, drawn as one flat list of the rows
   * that are visible, each indented by how deep it sits. A Folder, and a
   * Page with something beneath it, folds and unfolds. Each row has the
   * Entry menu behind its ⋯ button and a right click. With Write, a row is
   * dragged onto the upper or lower edge of another to go before or after
   * it, or onto its middle to go beneath it.
   */
  import { untrack } from 'svelte'
  import { entryMenuItems } from '../lib/entryMenu'
  import { dropPlace, moveEntry, type DropZone } from '../lib/move'
  import { navigation } from '../lib/navigation.svelte'
  import type { Area, Entry } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { above, visibleRows } from '../lib/tree'
  import { reportFailure } from '../lib/toasts'
  import { collapsed } from '../lib/ui.svelte'
  import EntryMenu from './EntryMenu.svelte'
  import EntryMenuButton, { openFromRow } from './EntryMenuButton.svelte'
  import Icon from './Icon.svelte'

  const {
    area,
    entries,
    currentId,
  }: {
    area: Area
    /** The top of the Area's tree, in order. */
    entries: Entry[]
    /** The Page that is open on the screen, if one is. */
    currentId: string | null
  } = $props()

  const rows = $derived(visibleRows(entries, collapsed))

  // Opening a Page unfolds what it sits beneath, so its row is there to be marked.
  // Only then: what someone folds afterwards stays folded.
  $effect(() => {
    const id = currentId
    if (id) untrack(() => above(entries, id).forEach(parent => collapsed.delete(parent)))
  })

  let menu: EntryMenu
  /** The Entry whose menu is open, so its row stays marked. */
  let menuFor = $state<string | null>(null)

  async function showMenu(entry: Entry, trigger: HTMLElement) {
    menuFor = entry.id
    await menu.open(entry, area, trigger)
  }

  /* ---- Dragging rows ---------------------------------------------------- */

  const draggable = $derived(area.role !== 'read')
  /** The Entry being dragged, and the row and edge it would be dropped on. */
  let dragged = $state.raw<Entry | null>(null)
  let dropOn = $state.raw<{ id: string; zone: DropZone } | null>(null)

  function dragStart(event: DragEvent, entry: Entry) {
    dragged = entry
    event.dataTransfer?.setData('text/plain', entry.title)
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
  }

  /** The upper and lower quarter of a row put the Entry before or after it; the middle, beneath it. */
  function zoneAt(event: DragEvent): DropZone {
    const row = (event.currentTarget as HTMLElement).getBoundingClientRect()
    const at = (event.clientY - row.top) / row.height
    return at < 0.25 ? 'before' : at > 0.75 ? 'after' : 'inside'
  }

  function dragOver(event: DragEvent, target: Entry) {
    if (!dragged) return
    const zone = zoneAt(event)
    if (!dropPlace(entries, dragged.id, target.id, zone)) {
      dropOn = null
      return
    }
    event.preventDefault()
    dropOn = { id: target.id, zone }
  }

  async function drop(event: DragEvent, target: Entry) {
    event.preventDefault()
    const entry = dragged
    const zone = zoneAt(event)
    dragEnd()
    const place = entry && dropPlace(entries, entry.id, target.id, zone)
    if (!entry || !place) return
    if (zone === 'inside') collapsed.delete(target.id)
    try {
      await moveEntry(entry, { area_id: area.id, ...place }, areaIds => navigation.reloadTrees(areaIds))
    } catch (caught) {
      reportFailure(caught, `${entry.title} could not be moved. Try again.`)
    }
  }

  function dragEnd() {
    dragged = null
    dropOn = null
  }

  function toggle(id: string) {
    if (collapsed.has(id)) collapsed.delete(id)
    else collapsed.add(id)
  }
</script>

<ul aria-label="Pages and Folders in {area.name}">
  {#each rows as { entry, depth, open } (entry.id)}
    {@const address = pageAddress(entry.id, entry.title)}
    <li
      class="row"
      class:current={entry.id === currentId}
      class:dragged={entry.id === dragged?.id}
      class:drop-before={dropOn?.id === entry.id && dropOn.zone === 'before'}
      class:drop-after={dropOn?.id === entry.id && dropOn.zone === 'after'}
      class:drop-inside={dropOn?.id === entry.id && dropOn.zone === 'inside'}
      style:--depth={depth}
      {draggable}
      ondragstart={event => dragStart(event, entry)}
      ondragover={event => dragOver(event, entry)}
      ondragleave={() => dropOn?.id === entry.id && (dropOn = null)}
      ondrop={event => drop(event, entry)}
      ondragend={dragEnd}
      oncontextmenu={event => openFromRow(event, trigger => showMenu(entry, trigger))}
    >
      {#if entry.children.length > 0}
        <button
          type="button"
          class="chevron"
          class:open
          aria-expanded={open}
          aria-label={open ? `Collapse ${entry.title}` : `Expand ${entry.title}`}
          onclick={() => toggle(entry.id)}
        >
          <Icon name="chevron" size={14} />
        </button>
      {:else}
        <span class="chevron" aria-hidden="true"></span>
      {/if}
      {#if entry.kind === 'folder'}
        <!-- The chevron says whether it is open; an empty Folder has nothing to open. -->
        <button type="button" class="label" tabindex="-1" disabled={entry.children.length === 0} onclick={() => toggle(entry.id)}>
          <span class="glyph"><Icon name="folder" size={15} /></span>
          <span class="title">{entry.title}</span>
        </button>
      {:else}
        <a
          class="label"
          draggable="false"
          href={address}
          aria-current={entry.id === currentId ? 'page' : undefined}
          onclick={event => router.follow(event, address)}
        >
          <span class="glyph"></span>
          <span class="title">{entry.title}</span>
        </a>
      {/if}
      {#if entryMenuItems(entry.kind, area.role).length > 0}
        <EntryMenuButton title={entry.title} open={menuFor === entry.id} onopen={trigger => showMenu(entry, trigger)} />
      {/if}
    </li>
  {/each}
</ul>

<EntryMenu bind:this={menu} onclose={() => (menuFor = null)} />

<style>
  ul {
    margin: 0;
    padding: 0 0 var(--space-2);
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    height: var(--height-row);
    padding-left: calc(var(--space-2) + var(--depth) * var(--indent));
    padding-right: var(--space-2);
    border-radius: var(--radius-s);
    color: var(--color-text-2);
    font-size: var(--text-m);
    transition: background var(--dur-fast) var(--ease-out);
  }

  .row:hover {
    background: var(--color-hover);
  }

  .row.current {
    background: var(--color-selected);
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .row.dragged {
    opacity: 0.5;
  }

  .row.drop-before {
    box-shadow: inset 0 var(--ring) 0 var(--color-focus);
  }

  .row.drop-after {
    box-shadow: inset 0 calc(-1 * var(--ring)) 0 var(--color-focus);
  }

  .row.drop-inside {
    background: var(--color-focus-soft);
    color: var(--color-text);
  }

  .chevron {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--size-chevron);
    height: var(--size-chevron);
    margin-right: var(--space-half);
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--color-muted);
  }

  button.chevron:hover {
    background: var(--color-line);
    color: var(--color-text);
  }

  button.chevron :global(svg) {
    transition: transform var(--dur-base) var(--ease-out);
  }

  button.chevron.open :global(svg) {
    transform: rotate(90deg);
  }

  .label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    height: 100%;
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    text-decoration: none;
  }

  button.label:disabled {
    opacity: 1;
  }

  .glyph {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--space-4);
    margin-right: var(--space-2);
    color: var(--color-muted);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
