<script lang="ts">
  import TokenManager from '../components/TokenManager.svelte'

  $effect(() => {
    document.title = 'Tokens · Turtle'
  })

  const mcpAddress = `${location.origin}/mcp`
</script>

<main class="screen">
  <h1>Tokens</h1>
  <p class="lede">
    A Token lets an Agent act as you, through MCP at <code>{mcpAddress}</code> or the HTTP API. It never has more access than you,
    and it stops working when you revoke it.
  </p>
  <TokenManager />
</main>
