<script lang="ts">
  import AppHeader from '../components/AppHeader.svelte'
  import TokenManager from '../components/TokenManager.svelte'

  $effect(() => {
    document.title = 'Tokens · Turtle'
  })

  const mcpAddress = `${location.origin}/mcp`
</script>

<AppHeader />

<main>
  <h1>Tokens</h1>
  <p class="muted">
    A Token lets an Agent act as you, through MCP at <code>{mcpAddress}</code> or the HTTP API. It never has more access than you,
    and it stops working when you revoke it.
  </p>
  <TokenManager />
</main>

<style>
  main {
    max-width: var(--width-form);
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  h1 {
    line-height: var(--leading-tight);
  }

  .muted {
    color: var(--color-muted);
    font-size: var(--text-m);
  }

  code {
    overflow-wrap: anywhere;
  }
</style>
