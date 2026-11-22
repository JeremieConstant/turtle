<script lang="ts">
  /*
   * The only signal about saving: every change goes over this connection as
   * it is typed, so while it holds, nothing is waiting to be saved.
   */
  import type { Status } from '../lib/connection'

  const { status }: { status: Status } = $props()

  const TEXT: Record<Status, string> = {
    connecting: 'Connecting…',
    connected: 'Connected',
    disconnected: 'Offline, retrying…',
  }
  const DETAIL: Record<Status, string> = {
    connecting: 'Changes are kept here until the connection is up.',
    connected: 'Every change is saved as it is made.',
    disconnected: 'Changes are kept here and saved once the connection is back.',
  }
</script>

<span class="status" data-state={status} title={DETAIL[status]} role="status">{TEXT[status]}</span>

<style>
  .status {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-s);
    white-space: nowrap;
  }

  .status::before {
    content: '';
    width: var(--space-2);
    height: var(--space-2);
    border-radius: var(--radius-round);
    background: var(--color-faint);
  }

  [data-state='connected']::before {
    background: var(--color-focus);
  }

  [data-state='disconnected'] {
    color: var(--color-danger);
  }

  [data-state='disconnected']::before {
    background: var(--color-danger);
  }
</style>
