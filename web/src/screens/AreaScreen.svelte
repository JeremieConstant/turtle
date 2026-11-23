<script lang="ts">
  /*
   * An Area on its own: its name, the Role the person holds there, and its
   * top-level Entries, each with the Entry menu. An empty Area offers the one
   * thing to do there, creating the first Page, to whoever may write.
   */
  import { untrack } from 'svelte'
  import EntryMenu from '../components/EntryMenu.svelte'
  import EntryMenuButton, { openFromRow } from '../components/EntryMenuButton.svelte'
  import Grants from '../components/Grants.svelte'
  import Icon from '../components/Icon.svelte'
  import NameDialog from '../components/NameDialog.svelte'
  import { toast } from 'svelte-sonner'
  import { entryMenuItems } from '../lib/entryMenu'
  import { navigation } from '../lib/navigation.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Area, Entry, Role } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { areaAddress, pageAddress } from '../lib/routes'
  import { ask } from '../lib/confirm.svelte'
  import { exportToFile } from '../lib/exporting'
  import { reportFailure } from '../lib/toasts'
  import { UNDO_DURATION } from '../lib/trash'
  import { createEntry } from '../lib/tree'
  import { collapsed, ui } from '../lib/ui.svelte'
  import NotFoundScreen from './NotFoundScreen.svelte'

  const ROLE_SENTENCES: Record<Role, string> = {
    read: 'Your Role here is Read: you can read this Area.',
    write: 'Your Role here is Write: you can read and change this Area.',
    admin: 'Your Role here is Admin: you can change this Area and its Grants.',
  }

  // The screen is created anew for every Area, so the ID never changes underneath it.
  const props: { id: string } = $props()
  const id = untrack(() => props.id)

  let area = $state.raw<Area | null>(null)
  /** Set when the Area does not exist or the caller may not read it; the two look the same. */
  let missing = $state(false)
  let loadFailed = $state(false)

  /** The top of the tree, in order: the same one the sidebar shows. */
  const entries = $derived(navigation.tree(id) ?? null)
  const writable = $derived(area !== null && area.role !== 'read')

  // The sidebar shows the tree of the Area that is on the screen.
  ui.openArea = id

  async function fetchTree() {
    area = (await navigation.loadTree(id)).area
  }

  fetchTree().catch(caught => {
    if (caught instanceof OperationError && caught.kind === 'not_found') missing = true
    else loadFailed = true
  })

  /** Loads the tree again after something in it changed. The change itself already happened. */
  async function reload() {
    try {
      await fetchTree()
    } catch (caught) {
      // Access may have been lost in the meantime; that looks like any other missing Area.
      if (caught instanceof OperationError && caught.kind === 'not_found') missing = true
      else toast.error('The change was saved, but the tree could not be loaded again. Reload the page to see it.')
    }
  }

  /** A Grant changed, and with it perhaps which Areas the person can read and what they may do there. */
  async function rolesChanged() {
    await Promise.all([reload(), navigation.refreshAreas()])
  }

  $effect(() => {
    if (area) document.title = `${area.name} · Turtle`
  })

  $effect(() => {
    if (!area) return
    // An address with an outdated or missing name is corrected in place (ADR-0007).
    const address = areaAddress(area.id, area.name)
    if (location.pathname !== address) router.replace(address)
  })

  let renameDialog = $state<NameDialog>()
  let firstPageDialog = $state<NameDialog>()
  let menu = $state<EntryMenu>()
  /** The Entry whose menu is open, so its row stays marked. */
  let menuFor = $state<string | null>(null)

  async function rename(name: string) {
    area = await call('rename_area', { id, name })
    void navigation.refreshAreas()
  }

  /*
   * Deleting takes the Area out of everyone's reach, the Admin's too, so the
   * toast's Undo is their only way back; afterwards a Workspace Admin
   * restores it from the settings.
   */
  async function deleteArea() {
    if (!area) return
    const { name } = area
    const yes = await ask({
      title: `Delete ${name}?`,
      message: `${name} and everything in it leave the sidebar of everyone. A Workspace Admin can restore it for 30 days, then it is gone for good.`,
      action: 'Delete Area',
      destructive: true,
    })
    if (!yes) return
    try {
      await call('delete_area', { id })
    } catch (caught) {
      reportFailure(caught, `${name} could not be deleted. Try again.`)
      return
    }
    await navigation.refreshAreas()
    router.navigate('/')
    toast(`Deleted ${name}`, {
      duration: UNDO_DURATION,
      action: {
        label: 'Undo',
        onClick: async () => {
          try {
            const restored = await call('restore_area', { id })
            await navigation.refreshAreas()
            router.navigate(areaAddress(restored.id, restored.name))
          } catch (caught) {
            reportFailure(caught, `${name} could not be restored. A Workspace Admin can restore it from the settings.`)
          }
        },
      },
    })
  }

  async function createFirstPage(title: string) {
    const page = await createEntry('page', { area_id: id, title })
    await reload()
    router.navigate(pageAddress(page.id, page.title))
  }

  /** A Folder has no screen of its own: it opens in the sidebar's tree. */
  function openFolder(entry: Entry) {
    ui.openArea = id
    collapsed.delete(entry.id)
  }

  async function showMenu(entry: Entry, trigger: HTMLElement) {
    if (!area) return
    menuFor = entry.id
    await menu?.open(entry, area, trigger)
  }

</script>

{#if missing}
  <NotFoundScreen />
{:else}
  <main>
    {#if loadFailed}
      <p class="sub">This Area could not be loaded. Reload the page to try again.</p>
    {:else if area}
      <div class="head">
        <h1>{area.name}</h1>
        <div class="actions">
          <button type="button" class="btn" onclick={() => area && exportToFile({ area_id: id }, area.name)}>Export</button>
          {#if area.role === 'admin' && !area.personal}
            <button type="button" class="btn" onclick={() => renameDialog?.open(area?.name)}>Rename</button>
            <button type="button" class="btn" onclick={deleteArea}>Delete</button>
          {/if}
        </div>
      </div>
      {#if area.personal}
        <p class="sub">Only you can see this Area. To share Pages, create them in another Area.</p>
      {:else}
        <p class="sub">{ROLE_SENTENCES[area.role]}</p>
      {/if}

      {#if entries && entries.length > 0}
        <ul class="entries" aria-label="Pages and Folders at the top of {area.name}">
          {#each entries as entry (entry.id)}
            <li class:menu-open={menuFor === entry.id} oncontextmenu={event => openFromRow(event, trigger => showMenu(entry, trigger))}>
              {#if entry.kind === 'page'}
                {@const address = pageAddress(entry.id, entry.title)}
                <a class="label" href={address} onclick={event => router.follow(event, address)}>
                  <span class="glyph"></span><span class="title">{entry.title}</span>
                </a>
              {:else}
                <button type="button" class="label" onclick={() => openFolder(entry)}>
                  <span class="glyph"><Icon name="folder" size={15} /></span><span class="title">{entry.title}</span>
                </button>
              {/if}
              {#if entryMenuItems(entry.kind, area.role).length > 0}
                <EntryMenuButton title={entry.title} open={menuFor === entry.id} onopen={trigger => showMenu(entry, trigger)} />
              {/if}
            </li>
          {/each}
        </ul>
      {:else if entries}
        <div class="empty">
          <p>This Area has no Pages yet.</p>
          {#if writable}
            <button type="button" class="btn primary" onclick={() => firstPageDialog?.open()}>
              <Icon name="plus" size={14} /> Create the first Page
            </button>
          {/if}
        </div>
      {/if}

      {#if area.role === 'admin' && !area.personal}
        <!-- Created anew whenever the Role changes, so it never shows a stale list. -->
        {#key area.role}
          <Grants areaId={id} onrolechange={rolesChanged} />
        {/key}
      {/if}
    {/if}
  </main>

  <EntryMenu bind:this={menu} onclose={() => (menuFor = null)} />
  <NameDialog bind:this={renameDialog} label="Name of the Area" action="Rename" onsubmit={rename} />
  <NameDialog bind:this={firstPageDialog} label="Title of the first Page" action="Create Page" onsubmit={createFirstPage} />
{/if}

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

  .actions {
    display: flex;
    gap: var(--space-2);
  }

  h1 {
    margin: 0;
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

  .entries {
    margin: 0 calc(-1 * var(--space-3)) var(--space-8);
    padding: 0;
    list-style: none;
  }

  .entries li {
    display: flex;
    align-items: center;
    height: var(--height-row-l);
    padding-right: var(--space-2);
    border-radius: var(--radius-m);
    transition: background var(--dur-fast) var(--ease-out);
  }

  .entries li:hover,
  .entries li.menu-open {
    background: var(--color-side);
  }

  .label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    height: 100%;
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-m);
    background: none;
    color: inherit;
    font-size: var(--text-base);
    font-weight: var(--weight-normal);
    text-align: left;
    text-decoration: none;
  }

  .glyph {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--space-4);
    margin-right: var(--space-3);
    color: var(--color-muted);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    display: grid;
    justify-items: start;
    gap: var(--space-4);
    margin-bottom: var(--space-8);
    padding: var(--space-8) 0;
    border-top: var(--line) solid var(--color-line);
  }

  .empty p {
    margin: 0;
    color: var(--color-text-2);
  }

  @media (max-width: 800px) {
    main {
      padding: var(--space-16) var(--space-4) var(--space-12);
    }
  }
</style>
