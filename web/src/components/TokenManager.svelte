<script lang="ts">
  import { untrack } from 'svelte'
  import { access, revokeOnceSure, type Revoking } from '../lib/connectedApps'
  import { call } from '../lib/ops'
  import type { TokenSummary } from '../lib/operations.gen'
  import { exactTime, nowInSeconds } from '../lib/time'
  import TokenForm from './TokenForm.svelte'

  /** The Tokens of a standalone Agent, which only a Workspace Admin manages. */
  const { agentId }: { agentId: string } = $props()
  const agent_id = untrack(() => agentId)

  let tokens = $state.raw<TokenSummary[] | null>(null)
  let loadFailed = $state(false)
  const revoking = $state<Revoking>({ busy: false, error: null })

  call('list_tokens', { agent_id })
    .then(list => (tokens = list.tokens))
    .catch(() => (loadFailed = true))

  async function revoke(token: TokenSummary) {
    const question = {
      title: `Revoke ${token.name}?`,
      message: 'The Agent loses access through it at once.',
      action: 'Revoke',
      destructive: true,
    }
    if (await revokeOnceSure(revoking, question, () => call('revoke_token', { id: token.id, agent_id }))) {
      tokens = tokens?.filter(known => known.id !== token.id) ?? null
    }
  }
</script>

{#if loadFailed}
  <p class="muted">The Tokens could not be loaded. Reload the page to try again.</p>
{:else if tokens !== null}
  <TokenForm {agentId} placeholder="Where it is used, e.g. CI pipeline" oncreated={token => (tokens = [token, ...(tokens ?? [])])} />

  {#if revoking.error}
    <p class="error section" role="alert">{revoking.error}</p>
  {/if}

  {#if tokens.length === 0}
    <p class="muted section">This Agent has no Tokens yet.</p>
  {:else}
    <ul class="list section">
      {#each tokens as token (token.id)}
        <li>
          <div class="item">
            <span class="item-name">{token.name}</span>
            <small class="hint">{access(token, nowInSeconds(), 'All Areas it has a Role in')}</small>
            <small class="hint">
              {token.last_used_at ? `Last used ${exactTime(token.last_used_at)}` : 'Never used'} · Created {exactTime(token.created_at)}
            </small>
          </div>
          <button type="button" class="btn" disabled={revoking.busy} onclick={() => revoke(token)}>Revoke</button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}
