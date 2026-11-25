<script lang="ts">
  import type { DemoAccount } from '../lib/operations.gen'
  import { OperationError } from '../lib/ops'

  /** Shows a demo's account and opens the demo with it in one click. */
  const {
    account,
    onopen,
  }: {
    account: DemoAccount
    /** Signs in with the account. A thrown OperationError is shown to the visitor. */
    onopen: () => Promise<void>
  } = $props()

  let busy = $state(false)
  let error = $state<string | null>(null)

  async function open() {
    busy = true
    error = null
    try {
      await onopen()
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

<section class="stack demo-sign-in" aria-label="Demo account">
  <dl class="credentials">
    <dt>Email address</dt>
    <dd>{account.email}</dd>
    <dt>Password</dt>
    <dd>{account.password}</dd>
  </dl>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  <button type="button" class="btn primary" disabled={busy} onclick={open}>Open the demo</button>
</section>

<style>
  .demo-sign-in {
    padding-bottom: var(--space-6);
    border-bottom: var(--line) solid var(--color-line);
  }

  /* Each label above its value, so a long address keeps the whole width of a narrow window. */
  .credentials {
    display: grid;
    gap: var(--space-half);
    margin: 0;
    font-size: var(--text-m);
  }

  .credentials dt {
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .credentials dd + dt {
    margin-top: var(--space-2);
  }

  .credentials dd {
    margin: 0;
    font-family: var(--font-mono);
    overflow-wrap: anywhere;
    user-select: all;
  }
</style>
