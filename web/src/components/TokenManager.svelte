<script lang="ts">
  import { untrack } from 'svelte'
  import { access, when } from '../lib/connectedApps'
  import { ask } from '../lib/confirm.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { TokenSummary } from '../lib/operations.gen'
  import TokenForm from './TokenForm.svelte'

  /** The Tokens of a standalone Agent, which only a Workspace Admin manages. */
  const { agentId }: { agentId: string } = $props()
  const agent_id = untrack(() => agentId)

  let tokens = $state.raw<TokenSummary[] | null>(null)
  let loadFailed = $state(false)
  let busy = $state(false)
  let error = $state<string | null>(null)

  call('list_tokens', { agent_id })
    .then(list => (tokens = list.tokens))
    .catch(() => (loadFailed = true))

  async function revoke(token: TokenSummary) {
    const sure = await ask({
      title: `Revoke ${token.name}?`,
      message: 'The Agent loses access through it at once.',
      action: 'Revoke',
      destructive: true,
    })
    if (!sure) return
    busy = true
    error = null
    try {
      await call('revoke_token', { id: token.id, agent_id })
      tokens = tokens?.filter(known => known.id !== token.id) ?? null
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

{#if loadFailed}
  <p class="muted">The Tokens could not be loaded. Reload the page to try again.</p>
{:else if tokens !== null}
  <TokenForm {agentId} placeholder="Where it is used, e.g. CI pipeline" oncreated={token => (tokens = [token, ...(tokens ?? [])])} />

  {#if error}
    <p class="error section" role="alert">{error}</p>
  {/if}

  {#if tokens.length === 0}
    <p class="muted section">This Agent has no Tokens yet.</p>
  {:else}
    <ul class="list section">
      {#each tokens as token (token.id)}
        <li>
          <div class="item">
            <span class="item-name">{token.name}</span>
            <small class="hint">{access(token, Date.now() / 1000, 'All Areas it has a Role in')}</small>
            <small class="hint">
              {token.last_used_at ? `Last used ${when(token.last_used_at)}` : 'Never used'} · Created {when(token.created_at)}
            </small>
          </div>
          <button type="button" class="btn" disabled={busy} onclick={() => revoke(token)}>Revoke</button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}
