<script lang="ts">
  import type { Snippet } from 'svelte'
  import { OperationError } from '../lib/ops'
  import Logo from './Logo.svelte'

  const {
    heading,
    intro,
    submitLabel,
    onsubmit,
    quietSubmit = false,
    lead,
    children,
  }: {
    heading: string
    intro: string
    submitLabel: string
    /** Runs the operation. A thrown OperationError is shown to the person. */
    onsubmit: () => Promise<void>
    /** Gives the submit button the quiet look, when something above the form is the main way in. */
    quietSubmit?: boolean
    /** Shown between the heading and the form. */
    lead?: Snippet
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

<main class="standalone">
  <div class="standalone-card">
    <div>
      <strong class="wordmark"><Logo size="var(--size-mark)" />Turtle</strong>
      <h1>{heading}</h1>
      <p class="muted">{intro}</p>
    </div>
    {@render lead?.()}
    <form class="stack" onsubmit={submit}>
      {@render children()}
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
      <button type="submit" class="btn" class:primary={!quietSubmit} disabled={busy}>{submitLabel}</button>
    </form>
  </div>
</main>
