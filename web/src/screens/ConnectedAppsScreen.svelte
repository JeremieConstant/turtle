<script lang="ts">
  /*
   * Everything that can act as the signed-in Member, in one list: apps they
   * connected through OAuth and Tokens they made by hand. Both are Tokens
   * underneath; disconnecting revokes it, and an app's access ends with it.
   */
  import TokenForm from '../components/TokenForm.svelte'
  import { access } from '../lib/connectedApps'
  import { ask } from '../lib/confirm.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { TokenSummary } from '../lib/operations.gen'
  import { exactTime, relativeTime } from '../lib/time'

  let apps = $state.raw<TokenSummary[] | null>(null)
  let loadFailed = $state(false)
  let busy = $state(false)
  let error = $state<string | null>(null)

  call('list_tokens', {})
    .then(list => (apps = list.tokens))
    .catch(() => (loadFailed = true))

  $effect(() => {
    document.title = 'Connected apps · Turtle'
  })

  const mcpAddress = `${location.origin}/mcp`
  const now = () => Math.floor(Date.now() / 1000)

  async function disconnect(app: TokenSummary) {
    const sure = await ask({
      title: `Disconnect ${app.name}?`,
      message: 'It stops working at once. To use it again, connect it again.',
      action: 'Disconnect',
      destructive: true,
    })
    if (!sure) return
    busy = true
    error = null
    try {
      await call('revoke_token', { id: app.id })
      apps = apps?.filter(known => known.id !== app.id) ?? null
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

<main class="screen">
  <h1>Connected apps</h1>
  <p class="lede">
    These can act as you. None of them ever has more access than you, and each stops working the moment you disconnect it.
  </p>

  {#if loadFailed}
    <p class="muted section">The connected apps could not be loaded. Reload the page to try again.</p>
  {:else if apps !== null}
    {#if error}
      <p class="error section" role="alert">{error}</p>
    {/if}

    {#if apps.length === 0}
      <p class="muted section">Nothing is connected yet.</p>
    {:else}
      <ul class="list section">
        {#each apps as app (app.id)}
          <li>
            <div class="item">
              <span class="item-name">{app.name}</span>
              <small class="hint">{access(app, now())}</small>
              <small class="hint">
                {#if app.last_used_at}
                  Last used <time title={exactTime(app.last_used_at)}>{relativeTime(app.last_used_at, now())}</time>
                {:else}
                  Never used
                {/if}
                · {app.connected ? 'Connected' : 'Token created'}
                <time title={exactTime(app.created_at)}>{relativeTime(app.created_at, now())}</time>
              </small>
            </div>
            <button type="button" class="btn" disabled={busy} onclick={() => disconnect(app)}>Disconnect</button>
          </li>
        {/each}
      </ul>
    {/if}

    <section class="section">
      <h2 class="section-title">For developers</h2>
      <p class="muted">
        For a script, or a client that cannot connect through OAuth: create a Token and send it to MCP at <code>{mcpAddress}</code> or
        to the HTTP API. It appears in the list above.
      </p>
      <div class="section">
        <TokenForm placeholder="What uses it, e.g. Backup script" oncreated={token => (apps = [token, ...(apps ?? [])])} />
      </div>
    </section>
  {/if}
</main>
