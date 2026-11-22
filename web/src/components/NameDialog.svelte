<script lang="ts">
  import { tick } from 'svelte'
  import { OperationError } from '../lib/ops'

  /** A small dialog that asks for one value and acts on it: the name of a new Area, Page or Folder, a new name for one, or a link address. */
  const {
    label,
    action,
    hint,
    optional = false,
    onsubmit,
  }: {
    /** What the value is for. */
    label: string
    /** What the button says. */
    action: string
    /** Shown under the field. */
    hint?: string
    /** Lets the button act on an empty value too, as when an empty link address removes the link. */
    optional?: boolean
    /** Runs the operation. If it fails the dialog stays open and says why. */
    onsubmit: (name: string) => Promise<void>
  } = $props()

  let dialog: HTMLDialogElement
  let input: HTMLInputElement
  let name = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)

  /** Opens the dialog. A value given here is filled in and selected, so typing replaces it. */
  export async function open(initial = '') {
    name = initial
    error = null
    dialog.showModal()
    // The name reaches the field once Svelte has updated it.
    await tick()
    input.select()
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    busy = true
    error = null
    try {
      await onsubmit(name.trim())
      dialog.close()
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

<dialog bind:this={dialog} closedby="any">
  <form class="dialog-body" onsubmit={submit}>
    <label>
      <span>{label}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:this={input} bind:value={name} autocomplete="off" autofocus />
      {#if hint}<small class="hint">{hint}</small>{/if}
    </label>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <div class="dialog-actions">
      <button type="button" class="btn" onclick={() => dialog.close()}>Cancel</button>
      <button type="submit" class="btn primary" disabled={busy || (!optional && !name.trim())}>{action}</button>
    </div>
  </form>
</dialog>

<style>
  label {
    display: grid;
    gap: var(--space-2);
    font-size: var(--text-s);
    font-weight: var(--weight-medium);
  }

  input {
    height: var(--space-8);
    font-size: var(--text-base);
  }

  input:focus {
    outline: none;
    border-color: var(--color-focus);
    box-shadow: 0 0 0 var(--ring) var(--color-focus-soft);
  }

  .error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-m);
  }
</style>
