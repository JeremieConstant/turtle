<script lang="ts">
  import type { Status } from '../lib/connection'

  const { status }: { status: Status } = $props()

  const TEXT: Record<Status, string> = {
    connecting: 'Connecting…',
    connected: 'Connected',
    disconnected: 'Offline, retrying…',
  }
</script>

<span class:connected={status === 'connected'} data-state={status}>{TEXT[status]}</span>

<style>
  span {
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .connected {
    color: var(--color-accent);
  }
</style>
