<script lang="ts">
  import { OperationError } from '../lib/ops'

  /** A small dialog that asks for one name and acts on it: a new Area, Page or Folder. */
  const {
    label,
    action,
    onsubmit,
  }: {
    /** What the name is for. */
    label: string
    /** What the button says. */
    action: string
    /** Runs the operation. If it fails the dialog stays open and says why. */
    onsubmit: (name: string) => Promise<void>
  } = $props()

  let dialog: HTMLDialogElement
  let name = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)

  export function open() {
    name = ''
    error = null
    dialog.showModal()
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
  <form onsubmit={submit}>
    <label>
      <span>{label}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={name} autocomplete="off" autofocus />
    </label>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <div class="actions">
      <button type="button" class="btn" onclick={() => dialog.close()}>Cancel</button>
      <button type="submit" class="btn primary" disabled={busy || !name.trim()}>{action}</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    width: min(var(--width-form), var(--width-floating-max));
  }

  dialog[open] {
    animation: pop var(--dur-base) var(--ease-out);
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(var(--space-1)) scale(var(--press-scale));
    }
  }

  form {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-4);
  }

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

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
