<script lang="ts">
  /*
   * The imports into an Area, newest first: where each came from, when and
   * by whom, how much of it is still there, a link to its report, and for
   * someone who may change all that it created a way to undo it.
   */
  import { importsOf, pagesAndFolders, REPORT_LABEL, startedBy, undoImport } from '../lib/imports'
  import { OperationError } from '../lib/ops'
  import type { Area, ImportRecord } from '../lib/operations.gen'
  import { pageAddress } from '../lib/routes'

  const { onundone }: { /** Called once an import was undone, to load the tree again. */ onundone?: () => void } = $props()

  let dialog: HTMLDialogElement
  let area = $state.raw<Area | null>(null)
  let imports = $state.raw<ImportRecord[] | null>(null)
  let error = $state<string | null>(null)
  let busy = $state(false)

  /** Opens the dialog for an Area and loads its imports. */
  export async function open(of: Area) {
    area = of
    imports = null
    error = null
    dialog.showModal()
    await load()
  }

  async function load() {
    if (!area) return
    try {
      imports = await importsOf(area.id)
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'The imports could not be loaded. Try again.'
    }
  }

  async function undo(record: ImportRecord) {
    busy = true
    try {
      if (await undoImport(record)) {
        onundone?.()
        await load()
      }
    } finally {
      busy = false
    }
  }

  /** When an import was started, by whom, and what is left of it. */
  function metaOf(record: ImportRecord): string {
    const parts = [when(record.started_at), startedBy(record.started_by)]
    parts.push(record.undone_at ? `Undone ${when(record.undone_at)}` : pagesAndFolders(record.created_count))
    return parts.filter(Boolean).join(' · ')
  }

  function when(seconds: number): string {
    return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
  }
</script>

<dialog bind:this={dialog} closedby="any" aria-labelledby="imports-title">
  <div class="dialog-body">
    <h2 id="imports-title">Imports into {area?.name ?? ''}</h2>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {:else if !imports}
      <p class="hint">Loading the imports…</p>
    {:else if imports.length === 0}
      <p class="hint">Nothing was imported into this Area yet.</p>
    {:else}
      <ul class="imports">
        {#each imports as record (record.id)}
          <li class="import">
            <div class="what">
              <span class="source">From {record.source}</span>
              <span class="meta">{metaOf(record)}</span>
            </div>
            <div class="actions">
              {#if record.report_page_id}
                <a class="btn" href={pageAddress(record.report_page_id, REPORT_LABEL)} onclick={() => dialog.close()}>Report</a>
              {/if}
              {#if record.undoable}
                <button type="button" class="btn" disabled={busy} onclick={() => undo(record)}>Undo</button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="dialog-actions">
      <button type="button" class="btn" onclick={() => dialog.close()}>Close</button>
    </div>
  </div>
</dialog>

<style>
  h2 {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .imports {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .import {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
  }

  .what {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .source {
    overflow-wrap: anywhere;
    font-size: var(--text-m);
    font-weight: var(--weight-medium);
  }

  .meta {
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .actions {
    display: flex;
    flex-shrink: 0;
    gap: var(--space-2);
  }

  .hint {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  .error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-m);
  }
</style>
