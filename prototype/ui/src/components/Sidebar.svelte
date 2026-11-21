<script lang="ts">
  /*
   * The navigation on every screen: the Workspace, a search field, and the
   * Areas the Member can read with the Personal Area first. The open Area
   * shows its tree beneath its name.
   */
  import { cubicOut } from 'svelte/easing'
  import { dur } from '../lib/motion'
  import { create_folder, create_page, get_session, get_tree, list_areas } from '../lib/mock/ops.svelte'
  import type { Area, TreeEntry } from '../lib/mock/types'
  import { router } from '../lib/router.svelte'
  import { areaAddress, pageAddress, searchAddress } from '../lib/routes'
  import { collapsed, ui } from '../lib/ui.svelte'
  import Icon from './Icon.svelte'
  import NameDialog from './NameDialog.svelte'
  import Tree from './Tree.svelte'

  let { currentPageId, showSearch }: { currentPageId: string | null; showSearch: boolean } = $props()

  const session = get_session()
  const areas = $derived(list_areas())
  let query = $state('')
  let folderDialog: NameDialog
  let folderArea = $state<Area | null>(null)

  /* A click opens the Area and shows its tree; a second click on the Area
     you are already looking at folds the tree away again. */
  function openArea(event: MouseEvent, area: Area) {
    const here = router.pathname.startsWith(`/a/${area.id}/`)
    if (ui.openArea === area.id && here) {
      event.preventDefault()
      ui.openArea = null
      return
    }
    ui.openArea = area.id
    router.follow(event, areaAddress(area.id, area.name))
    ui.drawerOpen = false
  }

  function parents(area: Area) {
    const out: string[] = []
    const walk = (list: TreeEntry[]) => list.forEach((e) => e.children.length && (out.push(e.id), walk(e.children)))
    walk(get_tree({ area_id: area.id }))
    return out
  }
  const collapseAll = (area: Area) => parents(area).forEach((id) => collapsed.add(id))
  const expandAll = (area: Area) => parents(area).forEach((id) => collapsed.delete(id))

  function newPage(area: Area) {
    const id = create_page({ area_id: area.id, title: 'Untitled' })
    ;(document.getElementById(`new-${area.id}`) as HTMLElement & { hidePopover(): void })?.hidePopover()
    router.navigate(pageAddress(id, 'Untitled') + '#rename')
    ui.drawerOpen = false
  }

  function newFolder(area: Area) {
    ;(document.getElementById(`new-${area.id}`) as HTMLElement & { hidePopover(): void })?.hidePopover()
    folderArea = area
    folderDialog.open()
  }

  function search(event: SubmitEvent) {
    event.preventDefault()
    router.navigate(searchAddress(query.trim()))
    query = ''
    ui.drawerOpen = false
  }

  function unfold(node: HTMLElement) {
    const h = node.offsetHeight
    return {
      duration: Math.min(dur('--dur-slow'), 120 + h / 6),
      easing: cubicOut,
      css: (t: number) => `height:${t * h}px;opacity:${Math.min(1, t * 1.5)};overflow:hidden`,
    }
  }
</script>

<nav class="side" aria-label="Areas">
  <div class="top">
    <a class="workspace" href="/" onclick={(e) => (router.follow(e, '/'), (ui.drawerOpen = false))}>
      <span class="mark" aria-hidden="true">N</span>
      {session.workspace.name}
    </a>
  </div>

  {#if showSearch}
    <form class="search" role="search" onsubmit={search}>
      <Icon name="search" size={14} />
      <input type="search" placeholder="Search" aria-label="Search all Areas" bind:value={query} />
      <kbd>/</kbd>
    </form>
  {/if}

  <div class="areas">
    {#each areas as area (area.id)}
      {@const open = ui.openArea === area.id}
      <section class="area" class:personal={area.personal} class:open>
        <div class="area-head">
          <a
            class="area-name"
            href={areaAddress(area.id, area.name)}
            aria-current={router.pathname.startsWith(`/a/${area.id}/`) ? 'page' : undefined}
            onclick={(e) => openArea(e, area)}
          >
            {area.name}
          </a>
          {#if open}
            <div class="tools">
              <button class="icon-btn" title="Collapse all" aria-label="Collapse all in {area.name}" onclick={() => collapseAll(area)}>
                <Icon name="collapse" size={14} />
              </button>
              <button class="icon-btn" title="Expand all" aria-label="Expand all in {area.name}" onclick={() => expandAll(area)}>
                <Icon name="expand" size={14} />
              </button>
              {#if area.role !== 'read'}
                <button class="icon-btn" title="New" aria-label="New in {area.name}" popovertarget="new-{area.id}" style:anchor-name="--new-{area.id}">
                  <Icon name="plus" size={14} />
                </button>
                <div class="menu" id="new-{area.id}" popover style:position-anchor="--new-{area.id}">
                  <button onclick={() => newPage(area)}>Page</button>
                  <button onclick={() => newFolder(area)}>Folder</button>
                </div>
              {/if}
            </div>
          {/if}
        </div>
        {#if open}
          <div class="tree" transition:unfold>
            {#if get_tree({ area_id: area.id }).length}
              <Tree {area} currentId={currentPageId} />
            {:else}
              <p class="empty">No Pages yet</p>
            {/if}
          </div>
        {/if}
      </section>
    {/each}
  </div>

  <div class="me">{session.member.name}</div>
</nav>

<NameDialog
  bind:this={folderDialog}
  label="Folder name"
  action="Create Folder"
  onsubmit={(title) => folderArea && create_folder({ area_id: folderArea.id, title })}
/>

<style>
  .side {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-side);
    border-right: var(--line) solid var(--color-line);
  }

  .top {
    display: flex;
    align-items: center;
    height: var(--height-bar);
    padding: 0 var(--space-4);
  }

  .workspace {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-m);
    font-weight: var(--weight-strong);
    letter-spacing: var(--tracking-heading);
    text-decoration: none;
  }

  .mark {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: var(--radius-s);
    background: var(--color-text);
    color: var(--color-bg);
    font-size: var(--text-xs);
    font-weight: var(--weight-strong);
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 30px;
    margin: 0 var(--space-3) var(--space-3);
    padding: 0 var(--space-2);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
    background: var(--color-surface);
    color: var(--color-muted);
    transition: border-color var(--dur-fast) var(--ease-out);
  }

  .search:focus-within {
    border-color: var(--color-focus);
    box-shadow: 0 0 0 3px var(--color-focus-soft);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--color-text);
    font-size: var(--text-m);
  }

  kbd {
    padding: 0 var(--space-1);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-s);
    font-family: var(--font-body);
    font-size: var(--text-xs);
    color: var(--color-faint);
  }

  .areas {
    flex: 1;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-4);
    scrollbar-width: thin;
  }

  .area.personal {
    margin-bottom: var(--space-3);
    padding-bottom: var(--space-3);
    border-bottom: var(--line) solid var(--color-line);
  }

  .area-head {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    height: 30px;
    background: var(--color-side);
  }

  .area-name {
    flex: 1;
    display: flex;
    align-items: center;
    height: 28px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-s);
    color: var(--color-text);
    font-size: var(--text-m);
    font-weight: var(--weight-medium);
    text-decoration: none;
    transition: background var(--dur-fast) var(--ease-out);
  }

  .area-name:hover {
    background: var(--color-hover);
  }

  .area-name[aria-current='page'] {
    background: var(--color-selected);
  }

  .area.open .area-name {
    font-weight: var(--weight-strong);
  }

  .tools {
    display: flex;
    gap: var(--space-half);
    padding-left: var(--space-1);
  }

  .menu {
    position: fixed;
    position-area: bottom span-left;
    margin-top: var(--space-1);
    min-width: 140px;
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

  .tree {
    padding-top: var(--space-half);
  }

  .empty {
    margin: var(--space-1) 0 var(--space-2);
    padding-left: calc(var(--space-2) + 20px);
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .me {
    padding: var(--space-3) var(--space-4);
    border-top: var(--line) solid var(--color-line);
    color: var(--color-muted);
    font-size: var(--text-s);
  }
</style>
