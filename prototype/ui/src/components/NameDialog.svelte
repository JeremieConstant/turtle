<script lang="ts">
  /* A small <dialog> that asks for one name, used for new Folders. */
  let { label, action, onsubmit }: { label: string; action: string; onsubmit: (value: string) => void } = $props()

  let dialog: HTMLDialogElement
  let value = $state('')

  export function open() {
    value = ''
    dialog.showModal()
  }

  function submit(event: SubmitEvent) {
    event.preventDefault()
    if (!value.trim()) return
    onsubmit(value.trim())
    dialog.close()
  }
</script>

<dialog bind:this={dialog} closedby="any">
  <form onsubmit={submit}>
    <label>
      <span>{label}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value autofocus />
    </label>
    <div class="actions">
      <button type="button" class="btn" onclick={() => dialog.close()}>Cancel</button>
      <button class="btn primary" disabled={!value.trim()}>{action}</button>
    </div>
  </form>
</dialog>

<style>
  dialog {
    width: min(360px, calc(100vw - 2 * var(--space-4)));
  }

  dialog[open] {
    animation: pop var(--dur-base) var(--ease-out);
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(4px) scale(0.98);
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
    height: 34px;
    padding: 0 var(--space-3);
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-m);
    font-size: var(--text-base);
  }

  input:focus {
    outline: none;
    border-color: var(--color-focus);
    box-shadow: 0 0 0 3px var(--color-focus-soft);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
