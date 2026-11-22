<script lang="ts">
  /*
   * The menu of an Entry, shared by the sidebar's tree and the Area screen:
   * a new Page or Folder beneath it, renaming it, and copying its link. What
   * it offers depends on the Role held on the Area (entryMenuItems). Creating
   * and renaming ask for the name in a dialog, then load the Area's tree
   * again, which both the sidebar and the Area screen show.
   */
  import { tick } from 'svelte'
  import { copyLink, entryMenuItems, type EntryMenuItem } from '../lib/entryMenu'
  import { menuKeys, openMenu } from '../lib/menu'
  import { navigation } from '../lib/navigation.svelte'
  import type { Area, Entry, Kind } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { pageAddress } from '../lib/routes'
  import { reportFailure } from '../lib/toasts'
  import { createEntry, renameEntry } from '../lib/tree'
  import { collapsed } from '../lib/ui.svelte'
  import NameDialog from './NameDialog.svelte'

  const LABELS: Record<EntryMenuItem, string> = {
    'new-page': 'New Page beneath…',
    'new-folder': 'New Folder beneath…',
    rename: 'Rename…',
    'copy-link': 'Copy link',
  }

  const { onclose }: { /** Called when the menu closes, however it closes. */ onclose?: () => void } = $props()

  let menu: HTMLDivElement
  let dialog: NameDialog
  /** The Entry the menu is open for, and the Area it is in. */
  let target = $state.raw<{ entry: Entry; area: Area } | null>(null)
  type DialogPurpose = Exclude<EntryMenuItem, 'copy-link'>
  /** What the dialog asks for, what its button says, and what it creates. */
  const DIALOGS: Record<DialogPurpose, { label: (title: string) => string; action: string; kind?: Kind }> = {
    'new-page': { label: title => `Title of the new Page beneath ${title}`, action: 'Create Page', kind: 'page' },
    'new-folder': { label: title => `Title of the new Folder beneath ${title}`, action: 'Create Folder', kind: 'folder' },
    rename: { label: title => `New title of ${title}`, action: 'Rename' },
  }
  let dialogPurpose = $state<DialogPurpose>('new-page')
  const shownDialog = $derived(DIALOGS[dialogPurpose])

  const items = $derived(target ? entryMenuItems(target.entry.kind, target.area.role) : [])

  /** Opens the menu of an Entry beside `trigger`, with its first item focused. */
  export async function open(entry: Entry, area: Area, trigger: HTMLElement) {
    target = { entry, area }
    await tick()
    openMenu(menu, trigger)
  }

  function choose(item: EntryMenuItem) {
    menu.hidePopover()
    if (!target) return
    if (item === 'copy-link') {
      void copyLink(target.entry)
      return
    }
    dialogPurpose = item
    dialog.open(item === 'rename' ? target.entry.title : '')
  }

  async function submit(title: string) {
    if (!target) return
    const { entry, area } = target
    const { kind } = shownDialog
    let created: Entry | null = null
    if (kind) created = await createEntry(kind, { area_id: area.id, parent_id: entry.id, title })
    else await renameEntry(entry, title)
    // The new Entry is shown where it went.
    if (created) collapsed.delete(entry.id)
    try {
      await navigation.loadTree(area.id)
    } catch (caught) {
      reportFailure(caught, 'The change was saved, but the tree could not be loaded again. Reload the page to see it.')
    }
    if (created?.kind === 'page') router.navigate(pageAddress(created.id, created.title))
  }

</script>

<div
  class="menu"
  role="menu"
  tabindex="-1"
  aria-label={target ? `Actions for ${target.entry.title}` : 'Actions'}
  popover
  bind:this={menu}
  onkeydown={menuKeys}
  ontoggle={event => event.newState === 'closed' && onclose?.()}
>
  {#each items as item, index (item)}
    {#if item === 'copy-link' && index > 0}
      <hr />
    {/if}
    <button type="button" class="menu-item" role="menuitem" onclick={() => choose(item)}>{LABELS[item]}</button>
  {/each}
</div>

<NameDialog bind:this={dialog} label={shownDialog.label(target?.entry.title ?? '')} action={shownDialog.action} onsubmit={submit} />

<style>
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
