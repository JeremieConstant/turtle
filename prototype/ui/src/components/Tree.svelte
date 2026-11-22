<script lang="ts">
  /*
   * The tree of one Area. It is drawn as one flat list of the visible rows,
   * each indented by its depth, so a move anywhere in the tree is a reorder
   * of one keyed list and every row can glide to its new place (FLIP).
   *
   * Dragging uses Pointer Events rather than HTML drag and drop, so the
   * preview follows the pointer 1:1 from where it was grabbed and the drop
   * can settle into its row. A drop calls move_entry, nothing else.
   *
   * A right click on a row, or its ⋯ button, opens the Entry menu: Edit,
   * Rename, Copy link, Move and Delete, each one operation.
   */
  import { tick } from 'svelte'
  import { flip } from 'svelte/animate'
  import { cubicOut } from 'svelte/easing'
  import { dur } from '../lib/motion'
  import { copyLink, trash } from '../lib/actions'
  import { get_tree, move_entry, rename_folder, rename_page } from '../lib/mock/ops.svelte'
  import type { Area, TreeEntry } from '../lib/mock/types'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { collapsed, ui } from '../lib/ui.svelte'
  import Icon from './Icon.svelte'
  import MoveDialog from './MoveDialog.svelte'

  let { area, currentId }: { area: Area; currentId: string | null } = $props()

  type Row = { entry: TreeEntry; depth: number; open: boolean }
  type Zone = 'before' | 'after' | 'inside'

  const tree = $derived(get_tree({ area_id: area.id }))
  const index = $derived.by(() => {
    const map = new Map<string, TreeEntry>()
    const walk = (list: TreeEntry[]) => list.forEach((e) => (map.set(e.id, e), walk(e.children)))
    walk(tree)
    return map
  })
  const rows = $derived.by(() => {
    const out: Row[] = []
    const walk = (list: TreeEntry[], depth: number) => {
      for (const entry of list) {
        const open = !collapsed.has(entry.id)
        out.push({ entry, depth, open })
        if (open) walk(entry.children, depth + 1)
      }
    }
    walk(tree, 0)
    return out
  })
  const depthOf = (id: string) => rows.find((r) => r.entry.id === id)?.depth ?? 0
  const writable = $derived(area.role !== 'read')

  function toggle(id: string) {
    if (collapsed.has(id)) collapsed.delete(id)
    else collapsed.add(id)
  }

  /* ---- Drag and drop -------------------------------------------------- */

  let list: HTMLDivElement
  let pressed: { id: string; x: number; y: number } | null = null
  let drag = $state<null | { id: string; title: string; folder: boolean; x: number; y: number; dx: number; dy: number; width: number }>(null)
  let drop = $state<null | { target: string; zone: Zone }>(null)
  let landing = $state<null | { x: number; y: number }>(null)
  let suppressClick = false
  let expandTimer: ReturnType<typeof setTimeout> | undefined

  function isWithin(id: string, ancestor: string) {
    let e = index.get(id)
    while (e) {
      if (e.id === ancestor) return true
      e = e.parent_id ? index.get(e.parent_id) : undefined
    }
    return false
  }

  function onpointerdown(event: PointerEvent, row: Row) {
    if (!writable || event.button !== 0 || event.pointerType === 'touch' || landing) return
    if ((event.target as Element).closest('.chev, .more, input')) return
    pressed = { id: row.entry.id, x: event.clientX, y: event.clientY }
  }

  function onpointermove(event: PointerEvent) {
    if (!pressed) return
    if (!drag) {
      // A few pixels of travel before it counts as a drag, so clicks stay clicks.
      if (Math.hypot(event.clientX - pressed.x, event.clientY - pressed.y) < 5) return
      const el = list.querySelector<HTMLElement>(`[data-row="${pressed.id}"]`)!
      const rect = el.getBoundingClientRect()
      const entry = index.get(pressed.id)!
      drag = {
        id: entry.id,
        title: entry.title,
        folder: entry.kind === 'folder',
        x: event.clientX,
        y: event.clientY,
        dx: pressed.x - rect.left,
        dy: pressed.y - rect.top,
        width: rect.width,
      }
      document.documentElement.classList.add('is-dragging')
    }
    drag.x = event.clientX
    drag.y = event.clientY
    drop = findDrop(event.clientX, event.clientY)
  }

  function findDrop(x: number, y: number): typeof drop {
    if (!drag) return null
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-row]')
    if (!el || !list.contains(el)) return drop
    const id = el.dataset.row!
    if (isWithin(id, drag.id)) return null
    const rect = el.getBoundingClientRect()
    const t = (y - rect.top) / rect.height
    let zone: Zone = t < 0.28 ? 'before' : t > 0.72 ? 'after' : 'inside'
    const entry = index.get(id)!
    // Below an open row with children means: first among those children.
    if (zone === 'after' && entry.children.length && !collapsed.has(id)) {
      const first = entry.children[0]
      if (first.id === drag.id) return null
      return { target: first.id, zone: 'before' }
    }
    // A collapsed row held as a drop target opens after a moment.
    clearTimeout(expandTimer)
    if (zone === 'inside' && collapsed.has(id)) expandTimer = setTimeout(() => collapsed.delete(id), 600)
    return { target: id, zone }
  }

  async function onpointerup() {
    pressed = null
    clearTimeout(expandTimer)
    if (!drag) return
    suppressClick = true
    setTimeout(() => (suppressClick = false))
    const target = drop && index.get(drop.target)
    const moving = index.get(drag.id)!
    if (!drop || !target) return cancel()

    let parent_id: string | null
    let at: number
    if (drop.zone === 'inside') {
      parent_id = target.id
      at = target.children.length
      collapsed.delete(target.id)
    } else {
      parent_id = target.parent_id
      at = target.position + (drop.zone === 'after' ? 1 : 0)
      if (moving.parent_id === parent_id && moving.position < at) at -= 1
    }
    drop = null
    move_entry({ entry_id: moving.id, area_id: area.id, parent_id, index: at })

    // Fly the preview into the row's new place, then let the row take over.
    await tick()
    const row = list.querySelector<HTMLElement>(`[data-row="${moving.id}"]`)
    const box = list.getBoundingClientRect()
    if (row) landing = { x: box.left, y: box.top + row.offsetTop - list.scrollTop }
    setTimeout(
      () => {
        finish()
        ui.settled = moving.id
        setTimeout(() => ui.settled === moving.id && (ui.settled = null), 900)
      },
      row ? dur('--dur-base') : 0,
    )
  }

  function cancel() {
    finish()
  }

  function finish() {
    drag = null
    drop = null
    landing = null
    pressed = null
    document.documentElement.classList.remove('is-dragging')
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && drag && !landing) cancel()
  }

  /* Where the line between rows is drawn, in the list's own coordinates. */
  const line = $derived.by(() => {
    if (!drop || drop.zone === 'inside' || !list) return null
    const el = list.querySelector<HTMLElement>(`[data-row="${drop.target}"]`)
    if (!el) return null
    return {
      top: el.offsetTop + (drop.zone === 'after' ? el.offsetHeight : 0),
      depth: depthOf(drop.target),
    }
  })

  /* ---- The Entry menu and renaming in place ---------------------------- */

  let menuEl = $state<HTMLDivElement>()
  let moveDialog: MoveDialog
  let menu = $state<null | { entry: TreeEntry; x: number; y: number }>(null)
  let renaming = $state<string | null>(null)
  let draft = $state('')

  /** A Folder in an Area the Member can only read has nothing to offer. */
  const hasMenu = (entry: TreeEntry) => writable || entry.kind === 'page'

  async function openMenu(entry: TreeEntry, x: number, y: number) {
    menu = { entry, x, y }
    await tick()
    if (!menuEl) return
    menuEl.showPopover()
    // Keep it on screen: open to the left or upwards when there is no room.
    const r = menuEl.getBoundingClientRect()
    menuEl.style.left = `${Math.max(8, Math.min(x, innerWidth - r.width - 8))}px`
    menuEl.style.top = `${y + r.height > innerHeight - 8 ? Math.max(8, y - r.height) : y}px`
    menuEl.querySelector<HTMLButtonElement>('button')?.focus()
  }

  function oncontextmenu(event: MouseEvent, entry: TreeEntry) {
    if (!hasMenu(entry) || drag) return
    event.preventDefault()
    openMenu(entry, event.clientX, event.clientY)
  }

  function onmore(event: MouseEvent, entry: TreeEntry) {
    const r = (event.currentTarget as HTMLElement).getBoundingClientRect()
    openMenu(entry, r.left, r.bottom + 4)
  }

  function run(action: (entry: TreeEntry) => void) {
    const entry = menu!.entry
    menuEl?.hidePopover()
    action(entry)
  }

  function edit(entry: TreeEntry) {
    ui.focusEditor = entry.id
    router.navigate(pageAddress(entry.id, entry.title))
    ui.drawerOpen = false
  }

  async function startRename(entry: TreeEntry) {
    renaming = entry.id
    draft = entry.title
    await tick()
    const input = list.querySelector<HTMLInputElement>('.rename')
    input?.focus()
    input?.select()
  }

  function saveRename(entry: TreeEntry) {
    if (renaming !== entry.id) return
    const title = draft.trim()
    renaming = null
    if (!title || title === entry.title) return
    if (entry.kind === 'page') rename_page({ page_id: entry.id, title })
    else rename_folder({ folder_id: entry.id, title })
  }

  function menuKeys(event: KeyboardEvent) {
    if (event.key === 'Escape') return menuEl?.hidePopover()
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
    event.preventDefault()
    const items = [...menuEl!.querySelectorAll<HTMLButtonElement>('button')]
    const at = items.indexOf(document.activeElement as HTMLButtonElement)
    items[(at + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus()
  }

  function follow(event: MouseEvent, entry: TreeEntry) {
    if (suppressClick) return event.preventDefault()
    router.follow(event, pageAddress(entry.id, entry.title))
    ui.drawerOpen = false
  }

  const glide = () => ({ duration: dur('--dur-slow'), easing: cubicOut })
  function appear(node: HTMLElement) {
    return { duration: dur('--dur-base'), easing: cubicOut, css: (t: number, u: number) => `opacity:${t};transform:translateY(${-u * 4}px)` }
  }
  function vanish(node: HTMLElement) {
    return { duration: dur('--dur-fast'), easing: cubicOut, css: (t: number) => `opacity:${t}` }
  }
</script>

<svelte:window {onpointermove} {onpointerup} {onkeydown} onblur={cancel} />

<div class="list" aria-label="Pages in {area.name}" role="group" bind:this={list}>
  {#each rows as row (row.entry.id)}
    {@const entry = row.entry}
    <div
      class="row"
      class:current={entry.id === currentId}
      class:inside={drop?.zone === 'inside' && drop.target === entry.id}
      class:lifted={drag?.id === entry.id}
      class:hidden={!!landing && drag?.id === entry.id}
      class:settled={ui.settled === entry.id}
      class:menu-open={menu?.entry.id === entry.id}
      class:writable
      style:--depth={row.depth}
      role="presentation"
      data-row={entry.id}
      onpointerdown={(e) => onpointerdown(e, row)}
      oncontextmenu={(e) => oncontextmenu(e, entry)}
      animate:flip={glide()}
      in:appear
      out:vanish
    >
      {#if entry.children.length}
        <button
          class="chev"
          class:open={row.open}
          aria-expanded={row.open}
          aria-label={row.open ? `Collapse ${entry.title}` : `Expand ${entry.title}`}
          onclick={() => toggle(entry.id)}
        >
          <Icon name="chevron" size={14} />
        </button>
      {:else}
        <span class="chev" aria-hidden="true"></span>
      {/if}
      {#if renaming === entry.id}
        <span class="label">
          <span class="glyph">{#if entry.kind === 'folder'}<Icon name="folder" size={15} />{/if}</span>
          <input
            class="rename"
            bind:value={draft}
            aria-label="Name"
            onblur={() => saveRename(entry)}
            onkeydown={(e) => {
              if (e.key === 'Enter') saveRename(entry)
              if (e.key === 'Escape') renaming = null
            }}
          />
        </span>
      {:else if entry.kind === 'folder'}
        <button class="label" onclick={() => !suppressClick && toggle(entry.id)}>
          <span class="glyph"><Icon name="folder" size={15} /></span>
          <span class="title">{entry.title}</span>
        </button>
      {:else}
        <a
          class="label"
          href={pageAddress(entry.id, entry.title)}
          aria-current={entry.id === currentId ? 'page' : undefined}
          onclick={(e) => follow(e, entry)}
          draggable="false"
        >
          <span class="glyph"></span>
          <span class="title">{entry.title}</span>
        </a>
      {/if}
      {#if hasMenu(entry) && renaming !== entry.id}
        <button class="more" aria-label="More for {entry.title}" aria-haspopup="menu" onclick={(e) => onmore(e, entry)}>
          <Icon name="more" size={14} />
        </button>
      {/if}
    </div>
  {/each}

  {#if line}
    <div class="line" style:top="{line.top}px" style:--depth={line.depth} aria-hidden="true"></div>
  {/if}
</div>

{#if menu}
  {@const entry = menu.entry}
  <div
    class="entry-menu"
    role="menu"
    tabindex="-1"
    aria-label="{entry.title}"
    popover
    bind:this={menuEl}
    ontoggle={(e) => e.newState === 'closed' && (menu = null)}
    onkeydown={menuKeys}
  >
    {#if entry.kind === 'page' && writable}
      <button role="menuitem" onclick={() => run(edit)}>Edit</button>
    {/if}
    {#if writable}
      <button role="menuitem" onclick={() => run(startRename)}>Rename</button>
    {/if}
    {#if entry.kind === 'page'}
      <button role="menuitem" onclick={() => run(copyLink)}>Copy link</button>
    {/if}
    {#if writable}
      <button role="menuitem" onclick={() => run((e) => moveDialog.open(e))}>Move…</button>
      <hr />
      <button role="menuitem" onclick={() => run((e) => trash(e, currentId))}>Delete</button>
    {/if}
  </div>
{/if}

<MoveDialog bind:this={moveDialog} />

{#if drag}
  <div
    class="ghost"
    class:landing={!!landing}
    style:width="{drag.width}px"
    style:transform={landing
      ? `translate(${landing.x}px, ${landing.y}px)`
      : `translate(${drag.x - drag.dx}px, ${drag.y - drag.dy}px)`}
    style:--depth={landing ? depthOf(drag.id) : 0}
    aria-hidden="true"
  >
    <span class="chev"></span>
    <span class="glyph">{#if drag.folder}<Icon name="folder" size={15} />{/if}</span>
    <span class="title">{drag.title}</span>
  </div>
{/if}

<style>
  .list {
    position: relative;
    padding-bottom: var(--space-2);
  }

  .row {
    position: relative;
    display: flex;
    align-items: center;
    height: var(--height-row);
    padding-left: calc(var(--space-2) + var(--depth) * var(--indent));
    padding-right: var(--space-2);
    border-radius: var(--radius-s);
    color: var(--color-text-2);
    font-size: var(--text-m);
    user-select: none;
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

  .row.writable {
    cursor: default;
  }

  /* While dragged, the row stays as a faint placeholder of where it was. */
  .row.lifted {
    opacity: 0.35;
  }

  .row.hidden {
    opacity: 0;
  }

  .row.inside {
    background: var(--color-focus-soft);
    box-shadow: inset 0 0 0 var(--line-thick) var(--color-focus);
    color: var(--color-text);
  }

  .row.settled {
    animation: settle var(--dur-agent) var(--ease-out);
  }

  @keyframes settle {
    from {
      background: var(--color-focus-soft);
    }
  }

  .chev {
    flex: none;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    margin-right: var(--space-half);
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--color-muted);
  }

  button.chev:hover {
    background: var(--color-line);
    color: var(--color-text);
  }

  button.chev :global(svg) {
    transform: rotate(0deg);
    transition: transform var(--dur-base) var(--ease-out);
  }

  button.chev.open :global(svg) {
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
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    text-decoration: none;
    cursor: inherit;
  }

  a.label {
    cursor: pointer;
  }

  .glyph {
    flex: none;
    display: grid;
    place-items: center;
    width: 16px;
    margin-right: var(--space-2);
    color: var(--color-muted);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The ⋯ button shows on hover, on the open Page, and while its menu is open. */
  .more {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-left: var(--space-1);
    padding: 0;
    border: 0;
    border-radius: var(--radius-s);
    background: transparent;
    color: var(--color-muted);
    opacity: 0;
    transition:
      opacity var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out);
  }

  .row:hover .more,
  .row.menu-open .more,
  .more:focus-visible {
    opacity: 1;
  }

  .more:hover,
  .row.menu-open .more {
    background: var(--color-line);
    color: var(--color-text);
  }

  :global(html.is-dragging) .more {
    opacity: 0;
  }

  .rename {
    flex: 1;
    min-width: 0;
    height: 22px;
    margin-left: calc(-1 * var(--space-1));
    padding: 0 var(--space-1);
    border: var(--line) solid var(--color-focus);
    border-radius: var(--radius-s);
    outline: 0;
    box-shadow: 0 0 0 3px var(--color-focus-soft);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-m);
  }

  .entry-menu {
    position: fixed;
    inset: auto;
    z-index: var(--layer-menu);
    min-width: 168px;
  }

  .entry-menu button {
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

  .entry-menu button:hover,
  .entry-menu button:focus-visible {
    background: var(--color-hover);
    outline: none;
  }

  .entry-menu hr {
    height: var(--line);
    margin: var(--space-1) var(--space-2);
    border: 0;
    background: var(--color-line);
  }

  .line {
    position: absolute;
    left: calc(var(--space-2) + var(--depth) * var(--indent) + 20px);
    right: var(--space-2);
    height: var(--line-thick);
    margin-top: -1px;
    border-radius: var(--line-thick);
    background: var(--color-focus);
    pointer-events: none;
    transition:
      top var(--dur-instant) var(--ease-out),
      left var(--dur-instant) var(--ease-out);
  }

  .line::before {
    content: '';
    position: absolute;
    left: -6px;
    top: -3px;
    width: 8px;
    height: 8px;
    box-sizing: border-box;
    border: var(--line-thick) solid var(--color-focus);
    border-radius: var(--radius-round);
    background: var(--color-side);
  }

  .ghost {
    position: fixed;
    top: 0;
    left: 0;
    z-index: var(--layer-drag);
    display: flex;
    align-items: center;
    height: var(--height-row);
    padding-left: calc(var(--space-2) + var(--depth) * var(--indent));
    padding-right: var(--space-3);
    border-radius: var(--radius-s);
    background: var(--color-surface);
    box-shadow: var(--shadow-float);
    color: var(--color-text);
    font-size: var(--text-m);
    font-weight: var(--weight-medium);
    pointer-events: none;
    scale: 1.02;
    transition: scale var(--dur-fast) var(--ease-out);
  }

  .ghost.landing {
    scale: 1;
    box-shadow: none;
    background: var(--color-selected);
    transition:
      transform var(--dur-base) var(--ease-out),
      scale var(--dur-base) var(--ease-out),
      box-shadow var(--dur-base) var(--ease-out),
      padding var(--dur-base) var(--ease-out),
      background var(--dur-base) var(--ease-out);
  }

  :global(html.is-dragging),
  :global(html.is-dragging *) {
    cursor: grabbing !important;
  }
</style>
