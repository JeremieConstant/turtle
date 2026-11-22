<script lang="ts">
  /*
   * The sidebar of every screen in the shell: the search, the Areas the
   * person can read with the tree of the open one beneath its name, and at
   * the foot the menu with the settings and signing out.
   */
  import { onDestroy, untrack } from 'svelte'
  import { cubicOut } from 'svelte/easing'
  import { menuKeys, openMenu } from '../lib/menu'
  import { dur } from '../lib/motion'
  import { navigation } from '../lib/navigation.svelte'
  import { call } from '../lib/ops'
  import type { Area, Kind } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { areaAddress, pageAddress } from '../lib/routes'
  import { session } from '../lib/session.svelte'
  import { reportFailure } from '../lib/toasts'
  import { createEntry, foldable } from '../lib/tree'
  import { collapsed, ui } from '../lib/ui.svelte'
  import Icon from './Icon.svelte'
  import NameDialog from './NameDialog.svelte'
  import SearchBox from './SearchBox.svelte'
  import SidebarTree from './SidebarTree.svelte'

  const route = $derived(router.route)
  const currentPageId = $derived(route.screen === 'page' ? route.id : null)
  const currentAreaId = $derived(route.screen === 'area' ? route.id : null)
  /** The Area the search covers unless it is widened: the one being looked at. */
  const searchAreaId = $derived(currentAreaId ?? (currentPageId ? ui.openArea : null) ?? undefined)

  void navigation.refreshAreas()
  // The next person to sign in here starts from nothing.
  onDestroy(() => {
    navigation.forget()
    collapsed.clear()
    ui.openArea = null
  })

  // The tree of the open Area is fetched once it is opened.
  $effect(() => {
    const id = ui.openArea
    if (id) untrack(() => void navigation.showTree(id))
  })

  /* A click opens the Area and shows its tree; a second click on the Area
     that is already on the screen folds the tree away again. */
  function openArea(event: MouseEvent, area: Area) {
    if (ui.openArea === area.id && currentAreaId === area.id) {
      event.preventDefault()
      ui.openArea = null
      return
    }
    router.follow(event, areaAddress(area.id, area.name))
    if (event.defaultPrevented) ui.openArea = area.id
  }

  const collapseAll = (area: Area) => foldable(navigation.tree(area.id) ?? []).forEach(id => collapsed.add(id))
  const expandAll = (area: Area) => foldable(navigation.tree(area.id) ?? []).forEach(id => collapsed.delete(id))

  /* ---- New Area, and new Page or Folder at the top of an Area ---------- */

  let areaDialog: NameDialog
  let entryDialog: NameDialog
  let newMenu: HTMLDivElement
  /** The Area the "New" menu was opened for, and what was picked there. */
  let newIn = $state.raw<Area | null>(null)
  let newKind = $state<Kind>('page')

  async function createArea(name: string) {
    const area = await call('create_area', { name })
    await navigation.refreshAreas()
    router.navigate(areaAddress(area.id, area.name))
  }

  function openNewMenu(event: MouseEvent, area: Area) {
    newIn = area
    openMenu(newMenu, event.currentTarget as HTMLElement)
  }

  function askForTitle(kind: Kind) {
    newKind = kind
    newMenu.hidePopover()
    entryDialog.open()
  }

  async function createAtTop(title: string) {
    const area = newIn
    if (!area) return
    const entry = await createEntry(newKind, { area_id: area.id, title })
    try {
      await navigation.loadTree(area.id)
    } catch (caught) {
      reportFailure(caught, 'The tree of this Area could not be loaded again. Reload the page to see the change.')
    }
    if (entry.kind === 'page') router.navigate(pageAddress(entry.id, entry.title))
  }

  /* ---- The menu at the foot --------------------------------------------- */

  let accountMenu: HTMLDivElement

  function go(event: MouseEvent, address: string) {
    router.follow(event, address)
    accountMenu.hidePopover()
  }

  async function signOut() {
    accountMenu.hidePopover()
    try {
      await session.signOut()
      router.navigate('/sign-in')
    } catch (caught) {
      reportFailure(caught, 'Signing out failed. Try again.')
    }
  }

  function unfold(node: HTMLElement) {
    const height = node.offsetHeight
    return {
      duration: dur('--dur-base'),
      easing: cubicOut,
      css: (t: number) => `height:${t * height}px;opacity:${t};overflow:hidden`,
    }
  }
</script>

<nav class="side" aria-label="Areas">
  <div class="top">
    <a class="brand" href="/" onclick={event => router.follow(event, '/')}>
      <span class="mark" aria-hidden="true">T</span>
      Turtle
    </a>
  </div>

  <!-- The start screen is a search field itself. -->
  {#if route.screen !== 'home'}
    <div class="search"><SearchBox areaId={searchAreaId} /></div>
  {/if}

  <div class="areas">
    {#if navigation.areas === null}
      {#if navigation.areasFailed}
        <p class="note">The Areas could not be loaded. Reload the page to try again.</p>
      {/if}
    {:else}
      {#if navigation.areas.length === 0}
        <p class="note">You cannot see any Areas yet.</p>
      {/if}
      {#each navigation.areas as area (area.id)}
        {@const open = ui.openArea === area.id}
        {@const tree = navigation.tree(area.id)}
        {@const address = areaAddress(area.id, area.name)}
        <section class="area" class:open>
          <div class="area-head">
            <a
              class="area-name"
              href={address}
              aria-current={currentAreaId === area.id ? 'page' : undefined}
              aria-expanded={open}
              onclick={event => openArea(event, area)}
            >
              <span class="title">{area.name}</span>
            </a>
            {#if open}
              <div class="tools">
                <button type="button" class="icon-btn" title="Collapse all" aria-label="Collapse all in {area.name}" onclick={() => collapseAll(area)}>
                  <Icon name="collapse" size={14} />
                </button>
                <button type="button" class="icon-btn" title="Expand all" aria-label="Expand all in {area.name}" onclick={() => expandAll(area)}>
                  <Icon name="expand" size={14} />
                </button>
                {#if area.role !== 'read'}
                  <button
                    type="button"
                    class="icon-btn"
                    title="New"
                    aria-label="New in {area.name}"
                    aria-haspopup="menu"
                    onclick={event => openNewMenu(event, area)}
                  >
                    <Icon name="plus" size={14} />
                  </button>
                {/if}
              </div>
            {/if}
          </div>
          {#if open && tree}
            <div class="tree" transition:unfold>
              {#if tree.length > 0}
                <SidebarTree {area} entries={tree} currentId={currentPageId} />
              {:else}
                <p class="note indented">No Pages yet</p>
              {/if}
            </div>
          {/if}
        </section>
      {/each}
      <button type="button" class="new-area" onclick={() => areaDialog.open()}>
        <Icon name="plus" size={14} /> New Area
      </button>
    {/if}
  </div>

  <div class="foot">
    <button
      type="button"
      class="account"
      aria-haspopup="menu"
      onclick={event => openMenu(accountMenu, event.currentTarget, 'above')}
    >
      <span class="title">{session.member?.name}</span>
      <Icon name="chevron" size={14} />
    </button>
  </div>
</nav>

<div class="menu" role="menu" tabindex="-1" aria-label={newIn ? `New in ${newIn.name}` : 'New'} popover bind:this={newMenu} onkeydown={menuKeys}>
  <button type="button" class="menu-item" role="menuitem" onclick={() => askForTitle('page')}>Page</button>
  <button type="button" class="menu-item" role="menuitem" onclick={() => askForTitle('folder')}>Folder</button>
</div>

<div class="menu" role="menu" tabindex="-1" aria-label="Settings and signing out" popover bind:this={accountMenu} onkeydown={menuKeys}>
  {#if session.member?.workspace_admin}
    <a class="menu-item" role="menuitem" href="/settings/members" onclick={event => go(event, '/settings/members')}>Members</a>
    <a class="menu-item" role="menuitem" href="/settings/agents" onclick={event => go(event, '/settings/agents')}>Agents</a>
  {/if}
  <a class="menu-item" role="menuitem" href="/settings/tokens" onclick={event => go(event, '/settings/tokens')}>Tokens</a>
  <hr />
  <button type="button" class="menu-item" role="menuitem" onclick={signOut}>Sign out</button>
</div>

<NameDialog bind:this={areaDialog} label="Name of the new Area" action="Create Area" onsubmit={createArea} />
<NameDialog
  bind:this={entryDialog}
  label={newKind === 'page' ? `Title of the new Page in ${newIn?.name}` : `Title of the new Folder in ${newIn?.name}`}
  action={newKind === 'page' ? 'Create Page' : 'Create Folder'}
  onsubmit={createAtTop}
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
    flex: none;
    display: flex;
    align-items: center;
    height: var(--height-bar);
    padding: 0 var(--space-4);
  }

  .brand {
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
    width: var(--size-mark);
    height: var(--size-mark);
    border-radius: var(--radius-s);
    background: var(--color-text);
    color: var(--color-bg);
    font-size: var(--text-xs);
  }

  .search {
    flex: none;
    margin: 0 var(--space-3) var(--space-3);
  }

  .areas {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-4);
    scrollbar-width: thin;
  }

  .area-head {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    height: var(--height-control);
    background: var(--color-side);
  }

  .area-name {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    height: var(--height-row);
    padding: 0 var(--space-2);
    border-radius: var(--radius-s);
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

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tools {
    display: flex;
    gap: var(--space-half);
    padding-left: var(--space-1);
  }

  .tree {
    padding-top: var(--space-half);
  }

  .note {
    margin: var(--space-1) 0 var(--space-2);
    padding: 0 var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .note.indented {
    padding-left: calc(var(--space-2) + var(--space-5));
  }

  .new-area {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: var(--height-row);
    margin-top: var(--space-2);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: var(--color-muted);
    font-size: var(--text-m);
    font-weight: var(--weight-normal);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }

  .new-area:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .foot {
    flex: none;
    padding: var(--space-2);
    border-top: var(--line) solid var(--color-line);
  }

  .account {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    height: var(--height-control);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-s);
    background: none;
    color: var(--color-text-2);
    font-size: var(--text-s);
    font-weight: var(--weight-normal);
    transition: background var(--dur-fast) var(--ease-out);
  }

  .account:hover {
    background: var(--color-hover);
  }

  .menu {
    position: fixed;
    inset: auto;
    z-index: var(--layer-menu);
    min-width: var(--width-menu-s);
  }

  .menu hr {
    height: var(--line);
    margin: var(--space-1) var(--space-2);
    border: 0;
    background: var(--color-line);
  }
</style>
