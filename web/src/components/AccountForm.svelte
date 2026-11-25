<script lang="ts">
  import type { Snippet } from 'svelte'
  import { Attempt } from '../lib/attempt.svelte'
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

  const attempt = new Attempt()

  function submit(event: SubmitEvent) {
    event.preventDefault()
    attempt.run(onsubmit)
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
      {#if attempt.error}
        <p class="error" role="alert">{attempt.error}</p>
      {/if}
      <button type="submit" class="btn" class:primary={!quietSubmit} disabled={attempt.busy}>{submitLabel}</button>
    </form>
  </div>
</main>
