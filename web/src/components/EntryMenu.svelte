<script lang="ts">
  /*
   * The menu of an Entry, shared by the sidebar's tree and the Area screen:
   * a new Page or Folder beneath it, renaming it, moving it elsewhere,
   * copying its link, exporting it as a PDF or as Markdown, and moving it to the Trash. What
   * it offers depends on the Role held on the Area (entryMenuItems). Creating
   * and renaming ask for the name in a dialog, then load the Area's tree
   * again, which both the sidebar and the Area screen show.
   */
  import { tick } from 'svelte'
  import { copyLink, entryMenuItems, type EntryMenuItem } from '../lib/entryMenu'
  import { exportToFile } from '../lib/exporting'
  import { menuKeys, openMenu } from '../lib/menu'
  import { navigation } from '../lib/navigation.svelte'
  import type { Area, Entry, Kind } from '../lib/operations.gen'
  import { router } from '../lib/router.svelte'
  import { areaAddress, pageAddress } from '../lib/routes'
  import { reportFailure } from '../lib/toasts'
  import { createEntry, findEntry, renameEntry } from '../lib/tree'
  import { moveToTrash } from '../lib/trash'
  import { collapsed } from '../lib/ui.svelte'
  import MoveDialog from './MoveDialog.svelte'
  import NameDialog from './NameDialog.svelte'
  import PdfExportDialog from './PdfExportDialog.svelte'

  const LABELS: Record<EntryMenuItem, string> = {
    'new-page': 'New Page beneath…',
    'new-folder': 'New Folder beneath…',
    rename: 'Rename…',
    move: 'Move…',
    'copy-link': 'Copy link',
    'export-pdf': 'Export as PDF…',
    'export-markdown': 'Export as Markdown',
    trash: 'Move to Trash',
  }

  const { onclose }: { /** Called when the menu closes, however it closes. */ onclose?: () => void } = $props()

  let menu: HTMLDivElement
  let dialog: NameDialog
  let moveDialog: MoveDialog
  /** The Entry the menu is open for, and the Area it is in. */
  let target = $state.raw<{ entry: Entry; area: Area } | null>(null)
  let pdfDialog: PdfExportDialog
  type DialogPurpose = Exclude<EntryMenuItem, 'move' | 'copy-link' | 'export-pdf' | 'export-markdown' | 'trash'>
  /** What the dialog asks for, what its button says, and what it creates. */
  const DIALOGS: Record<DialogPurpose, { label: (title: string) => string; action: string; kind?: Kind }> = {
    'new-page': { label: title => `Title of the new Page beneath ${title}`, action: 'Create Page', kind: 'page' },
    'new-folder': { label: title => `Title of the new Folder beneath ${title}`, action: 'Create Folder', kind: 'folder' },
    rename: { label: title => `New title of ${title}`, action: 'Rename' },
  }
  let dialogPurpose = $state<DialogPurpose>('new-page')
  const shownDialog = $derived(DIALOGS[dialogPurpose])

  /** Items are shown in groups with a line between them: changing it, passing it on, and the Trash. */
  const groupOf = (item: EntryMenuItem) =>
    item === 'trash' ? 2 : item === 'copy-link' || item === 'export-pdf' || item === 'export-markdown' ? 1 : 0

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
    if (item === 'export-pdf') {
      const { entry, area } = target
      void pdfDialog.open(area.id, entry.id, entry.title)
      return
    }
    if (item === 'export-markdown') {
      const { entry, area } = target
      void exportToFile({ area_id: area.id, entry_id: entry.id }, entry.title)
      return
    }
    if (item === 'move') {
      moveDialog.open(target.entry, target.area)
      return
    }
    if (item === 'trash') {
      void trash(target.entry, target.area)
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

  /** Moves an Entry to the Trash; a Page on the screen that went with it gives way to its Area. */
  async function trash(entry: Entry, area: Area) {
    const route = router.route
    const onScreen = route.screen === 'page' && (route.id === entry.id || findEntry(entry.children, route.id) !== undefined)
    try {
      await moveToTrash(entry, area, async () => {
        try {
          await navigation.loadTree(area.id)
        } catch (caught) {
          reportFailure(caught, 'The tree of this Area could not be loaded again. Reload the page to see the change.')
        }
      })
    } catch (caught) {
      reportFailure(caught, `${entry.title} could not be moved to the Trash. Try again.`)
      return
    }
    if (onScreen) router.navigate(areaAddress(area.id, area.name))
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
    {#if index > 0 && groupOf(item) !== groupOf(items[index - 1])}
      <hr />
    {/if}
    <button type="button" class="menu-item" role="menuitem" onclick={() => choose(item)}>{LABELS[item]}</button>
  {/each}
</div>

<MoveDialog bind:this={moveDialog} />

<PdfExportDialog bind:this={pdfDialog} />

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
