<script lang="ts">
  import { untrack } from 'svelte'
  import { holderFromKey, holderKey, holderLabel, withoutGrant } from '../lib/grants'
  import { call, OperationError } from '../lib/ops'
  import type { Grant, GrantHolder, GrantList, Role, RoleAfterwards } from '../lib/operations.gen'

  const {
    areaId,
    onrolechange,
  }: {
    /** An Area the caller is Admin of. */
    areaId: string
    /** Called once a change leaves the caller without the Admin Role, so the screen can load the Area again. */
    onrolechange: () => void
  } = $props()
  const id = untrack(() => areaId)

  const ROLES: Role[] = ['read', 'write', 'admin']
  const ROLE_NAMES: Record<Role, string> = { read: 'Read', write: 'Write', admin: 'Admin' }

  let list = $state.raw<GrantList | null>(null)
  let loadFailed = $state(false)
  let busy = $state(false)
  let error = $state<string | null>(null)
  let newHolder = $state('')
  let newRole = $state<Role>('read')

  const choices = $derived(list ? withoutGrant(list) : [])

  async function load() {
    list = await call('list_grants', { area_id: id })
    if (!choices.some(choice => choice.key === newHolder)) newHolder = choices[0]?.key ?? ''
  }

  load().catch(() => (loadFailed = true))

  /**
   * Runs a change of Grants and shows the list again, or hands over once the
   * caller is no longer Admin. Says whether the change was made.
   */
  async function change(run: () => Promise<RoleAfterwards>): Promise<boolean> {
    busy = true
    error = null
    try {
      const afterwards = await run()
      if (afterwards.role !== 'admin') onrolechange()
      else await load()
      return true
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
      return false
    } finally {
      busy = false
    }
  }

  function setRole(holder: GrantHolder, role: Role) {
    return change(() => call('set_grant', { area_id: id, holder, role }))
  }

  /** A Role chosen in the list; a refused change shows the Role held before again. */
  async function choose(grant: Grant, select: HTMLSelectElement) {
    if (!(await setRole(grant.holder, select.value as Role))) select.value = grant.role
  }

  function remove(grant: Grant) {
    return change(() => call('remove_grant', { area_id: id, holder: grant.holder }))
  }

  function add(event: SubmitEvent) {
    event.preventDefault()
    if (newHolder) setRole(holderFromKey(newHolder), newRole)
  }
</script>

<section class="section stack">
  <h2 class="section-title">Who has access</h2>
  {#if loadFailed}
    <p class="muted">Access could not be loaded. Reload the page to try again.</p>
  {:else if list}
    <ul class="list">
      {#each list.grants as grant (holderKey(grant.holder))}
        <li>
          <span class="item-name">{holderLabel(grant.holder, grant.name)}</span>
          <div class="controls">
            <select
              class="input"
              aria-label="Role of {holderLabel(grant.holder, grant.name)}"
              disabled={busy}
              value={grant.role}
              onchange={event => choose(grant, event.currentTarget)}
            >
              {#each ROLES as role}
                <option value={role}>{ROLE_NAMES[role]}</option>
              {/each}
            </select>
            <button type="button" class="btn" disabled={busy} onclick={() => remove(grant)}>Remove</button>
          </div>
        </li>
      {/each}
    </ul>
    {#if choices.length > 0}
      <form class="controls" onsubmit={add}>
        <select class="input grow" aria-label="Whom to give a Role" bind:value={newHolder}>
          {#each choices as choice (choice.key)}
            <option value={choice.key}>{choice.label}</option>
          {/each}
        </select>
        <select class="input" aria-label="Role to give" bind:value={newRole}>
          {#each ROLES as role}
            <option value={role}>{ROLE_NAMES[role]}</option>
          {/each}
        </select>
        <button type="submit" class="btn primary" disabled={busy}>Give Role</button>
      </form>
    {/if}
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  {/if}
</section>
