<script lang="ts">
  import { Attempt } from '../lib/attempt.svelte'
  import type { Credentials } from '../lib/operations.gen'

  /** Shows the email address and password of a Demo's demo Member and opens the Demo with them in one click. */
  const {
    account,
    onopen,
  }: {
    account: Credentials
    /** Signs in as the demo Member. A thrown OperationError is shown to the person. */
    onopen: () => Promise<void>
  } = $props()

  const attempt = new Attempt()
</script>

<section class="stack demo-sign-in" aria-label="Demo account">
  <dl class="credentials">
    <dt>Email address</dt>
    <dd>{account.email}</dd>
    <dt>Password</dt>
    <dd>{account.password}</dd>
  </dl>
  {#if attempt.error}
    <p class="error" role="alert">{attempt.error}</p>
  {/if}
  <button type="button" class="btn primary" disabled={attempt.busy} onclick={() => attempt.run(onopen)}>Open the demo</button>
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
