<script lang="ts">
  import { answer, confirmation } from '../lib/confirm.svelte'

  /** Shows the question ask() put up, once, for the whole app. */
  let dialog: HTMLDialogElement

  $effect(() => {
    if (confirmation.question && !dialog.open) dialog.showModal()
    else if (!confirmation.question && dialog.open) dialog.close()
  })
</script>

<!-- Escape, a click beside it and Cancel all close it, which answers no. -->
<dialog bind:this={dialog} closedby="any" aria-labelledby="confirm-title" onclose={() => answer(false)}>
  {#if confirmation.question}
    {@const question = confirmation.question}
    <div class="dialog-body">
      <h2 id="confirm-title">{question.title}</h2>
      <p class="muted">{question.message}</p>
      <div class="dialog-actions">
        <!-- svelte-ignore a11y_autofocus -->
        <button type="button" class="btn" autofocus onclick={() => dialog.close()}>Cancel</button>
        <button type="button" class="btn {question.destructive ? 'danger' : 'primary'}" onclick={() => answer(true)}>
          {question.action}
        </button>
      </div>
    </div>
  {/if}
</dialog>
