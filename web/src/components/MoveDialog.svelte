<script lang="ts">
  /*
   * Asks where an Entry goes: an Area where the person holds Write, and in
   * its tree the top or an Entry to go beneath, never the Entry itself or
   * what is beneath it. It goes last there; the toast offers to move it back.
   */
  import { moveEntry, placesBeneath, writableAreas } from '../lib/move'
  import { navigation } from '../lib/navigation.svelte'
  import { OperationError } from '../lib/ops'
  import type { Area, Entry } from '../lib/operations.gen'

  let dialog: HTMLDialogElement
  let moving = $state.raw<Entry | null>(null)
  let areaId = $state('')
  /** The Entry to go beneath; '' for the top of the Area. */
  let parentId = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)

  const areas = $derived(writableAreas(navigation.areas ?? []))
  const tree = $derived(navigation.tree(areaId))
  const places = $derived(moving && tree ? placesBeneath(tree, moving.id) : [])
  const areaName = $derived(areas.find(area => area.id === areaId)?.name ?? '')

  $effect(() => {
    if (areaId) void navigation.showTree(areaId)
  })

  /** Opens the dialog for an Entry, with the Area it is in chosen. */
  export function open(entry: Entry, area: Area) {
    moving = entry
    areaId = area.id
    parentId = ''
    error = null
    if (!navigation.areas) void navigation.refreshAreas()
    dialog.showModal()
  }

  function chooseArea() {
    parentId = ''
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    if (!moving) return
    const entry = moving
    busy = true
    error = null
    try {
      await moveEntry(entry, { area_id: areaId, parent_id: parentId || undefined }, areaIds =>
        navigation.reloadTrees(areaIds),
      )
      dialog.close()
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

<dialog bind:this={dialog} closedby="any" aria-labelledby="move-title">
  <form class="dialog-body" onsubmit={submit}>
    <h2 id="move-title">Move {moving?.title ?? ''}</h2>
    <label class="field">
      <span>Area</span>
      <select bind:value={areaId} onchange={chooseArea}>
        {#each areas as area (area.id)}
          <option value={area.id}>{area.name}</option>
        {/each}
      </select>
    </label>
    <fieldset>
      <legend>Place</legend>
      <div class="places">
        <label class="place">
          <input type="radio" name="place" value="" bind:group={parentId} />
          <span>Top of {areaName}</span>
        </label>
        {#each places as { entry, depth } (entry.id)}
          <label class="place" style:--depth={depth + 1}>
            <input type="radio" name="place" value={entry.id} bind:group={parentId} />
            <span class="title">{entry.title}</span>
          </label>
        {/each}
        {#if !tree}
          <p class="hint">Loading the tree…</p>
        {/if}
      </div>
    </fieldset>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <div class="dialog-actions">
      <button type="button" class="btn" onclick={() => dialog.close()}>Cancel</button>
      <button type="submit" class="btn primary" disabled={busy || !areaId}>Move</button>
    </div>
  </form>
</dialog>

<style>
  h2 {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .field {
    display: grid;
    gap: var(--space-2);
    font-size: var(--text-s);
    font-weight: var(--weight-medium);
  }

  select {
    height: var(--space-8);
    font-size: var(--text-base);
  }

  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: var(--space-2);
    padding: 0;
    font-size: var(--text-s);
    font-weight: var(--weight-medium);
  }

  .places {
    max-height: var(--height-places-max);
    overflow-y: auto;
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-m);
  }

  .place {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    height: var(--height-row);
    padding-left: calc(var(--space-2) + var(--depth, 0) * var(--indent));
    padding-right: var(--space-2);
    font-size: var(--text-m);
  }

  .place:hover {
    background: var(--color-hover);
  }

  .place:has(input:checked) {
    background: var(--color-selected);
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint {
    margin: 0;
    padding: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-m);
  }
</style>
