<script lang="ts">
  /*
   * The only signal about saving: every change goes over this connection as
   * it is typed, so while it holds, nothing is waiting to be saved. Holding
   * is the normal case and says nothing; the note shows only when it does
   * not, and arrives with a little movement so it is noticed.
   */
  import { fade } from 'svelte/transition'
  import type { Status } from '../lib/connection'

  const { status }: { status: Status } = $props()

  const TEXT: Record<Exclude<Status, 'connected'>, string> = {
    connecting: 'Connecting…',
    disconnected: 'Offline, retrying…',
  }
  const DETAIL: Record<Exclude<Status, 'connected'>, string> = {
    connecting: 'Changes are kept here until the connection is up.',
    disconnected: 'Changes are kept here and saved once the connection is back.',
  }
</script>

<!-- The live region stays in place so that a note put into it is announced. -->
<span class="region" role="status">
  {#if status !== 'connected'}
    <span class="status" data-state={status} title={DETAIL[status]} out:fade={{ duration: 140 }}>{TEXT[status]}</span>
  {/if}
</span>

<style>
  .region {
    display: contents;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-s);
    white-space: nowrap;
    /* Waits a moment first: a Page that connects at once never shows it. */
    animation: arrive var(--dur-slow) var(--ease-out) var(--delay-notice) backwards;
  }

  .status::before {
    content: '';
    width: var(--space-2);
    height: var(--space-2);
    border-radius: var(--radius-round);
    background: var(--color-faint);
    animation: ping calc(var(--dur-slow) * 2) var(--ease-out) calc(var(--delay-notice) + var(--dur-slow)) backwards;
  }

  [data-state='disconnected'] {
    color: var(--color-danger);
  }

  [data-state='disconnected']::before {
    background: var(--color-danger);
  }

  /* Slides in from where the header's buttons sit and settles without overshoot. */
  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateX(var(--space-2)) scale(0.96);
    }
  }

  /* One swell of the dot once the note has landed: a nudge, never a loop. */
  @keyframes ping {
    40% {
      transform: scale(1.8);
    }
  }
</style>
