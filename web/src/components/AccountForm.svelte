<script lang="ts">
  import type { Snippet } from 'svelte'
  import { OperationError } from '../lib/ops'

  const {
    heading,
    intro,
    submitLabel,
    onsubmit,
    children,
  }: {
    heading: string
    intro: string
    submitLabel: string
    /** Runs the operation. A thrown OperationError is shown to the person. */
    onsubmit: () => Promise<void>
    children: Snippet
  } = $props()

  let busy = $state(false)
  let error = $state<string | null>(null)

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    busy = true
    error = null
    try {
      await onsubmit()
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

<main>
  <strong class="brand">Turtle</strong>
  <h1>{heading}</h1>
  <p class="intro">{intro}</p>
  <form onsubmit={submit}>
    {@render children()}
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <button type="submit" disabled={busy}>{submitLabel}</button>
  </form>
</main>

<style>
  main {
    max-width: var(--width-form);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  .brand {
    color: var(--color-focus);
  }

  h1 {
    line-height: var(--leading-tight);
  }

  .intro {
    color: var(--color-muted);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .error {
    margin: 0;
    color: var(--color-danger);
  }

  button {
    align-self: flex-start;
  }
</style>
