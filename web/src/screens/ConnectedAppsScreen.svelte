<script lang="ts">
  /*
   * Everything that can act as the signed-in Member, in one list: apps they
   * connected through OAuth and Tokens they made by hand. Both are Tokens
   * underneath; disconnecting revokes it, and an app's access ends with it.
   */
  import TokenForm from '../components/TokenForm.svelte'
  import { router } from '../lib/router.svelte'
  import { access, revokeOnceSure, type Revoking } from '../lib/connectedApps'
  import { call } from '../lib/ops'
  import type { TokenSummary } from '../lib/operations.gen'
  import { exactTime, nowInSeconds as now, relativeTime } from '../lib/time'

  let apps = $state.raw<TokenSummary[] | null>(null)
  let loadFailed = $state(false)
  const revoking = $state<Revoking>({ busy: false, error: null })

  call('list_tokens', {})
    .then(list => (apps = list.tokens))
    .catch(() => (loadFailed = true))

  $effect(() => {
    document.title = 'Connected apps · Turtle'
  })

  let forDevelopers = $state<HTMLElement | null>(null)

  // The Connect AI page links straight to creating a Token by hand.
  $effect(() => {
    if (forDevelopers && location.hash === '#for-developers') forDevelopers.scrollIntoView({ block: 'start' })
  })

  const mcpAddress = `${location.origin}/mcp`

  async function disconnect(app: TokenSummary) {
    const question = {
      title: `Disconnect ${app.name}?`,
      message: 'It stops working at once. To use it again, connect it again.',
      action: 'Disconnect',
      destructive: true,
    }
    if (await revokeOnceSure(revoking, question, () => call('revoke_token', { id: app.id }))) {
      apps = apps?.filter(known => known.id !== app.id) ?? null
    }
  }
</script>

<main class="screen">
  <h1>Connected apps</h1>
  <p class="lede">
    These can act as you. None of them ever has more access than you, and each stops working the moment you disconnect it. To
    connect Claude or another AI client, see
    <a href="/settings/connect-ai" onclick={event => router.follow(event, '/settings/connect-ai')}>Connect AI</a>.
  </p>

  {#if loadFailed}
    <p class="muted section">The connected apps could not be loaded. Reload the page to try again.</p>
  {:else if apps !== null}
    {#if revoking.error}
      <p class="error section" role="alert">{revoking.error}</p>
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
            <button type="button" class="btn" disabled={revoking.busy} onclick={() => disconnect(app)}>Disconnect</button>
          </li>
        {/each}
      </ul>
    {/if}

    <section class="section" id="for-developers" bind:this={forDevelopers}>
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
