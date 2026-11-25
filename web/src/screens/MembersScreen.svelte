<script lang="ts">
  import ProfilePicture from '../components/ProfilePicture.svelte'
  import { ask } from '../lib/confirm.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { MemberEntry, SingleUseLink } from '../lib/operations.gen'
  import { session } from '../lib/session.svelte'

  let members = $state.raw<MemberEntry[] | null>(null)
  let loadError = $state<string | null>(null)
  let email = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)
  /** The last link created, with what it is for, to pass on. */
  let shown = $state.raw<{ purpose: string; link: SingleUseLink } | null>(null)
  let copied = $state(false)

  call('list_members', {})
    .then(list => (members = list.members))
    .catch(caught => {
      loadError =
        caught instanceof OperationError && caught.kind === 'not_permitted'
          ? 'Only a Workspace Admin can manage Members.'
          : 'The Members could not be loaded. Reload the page to try again.'
    })

  $effect(() => {
    document.title = 'Members · Turtle'
  })

  /** Runs an operation, showing what went wrong if it fails. */
  async function attempt(run: () => Promise<void>) {
    busy = true
    error = null
    try {
      await run()
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }

  function show(purpose: string, link: SingleUseLink) {
    shown = { purpose, link }
    copied = false
  }

  async function invite(event: SubmitEvent) {
    event.preventDefault()
    await attempt(async () => {
      const link = await call('invite_member', { email })
      show(`Invitation for ${email.trim()}`, link)
      email = ''
    })
  }

  /** Puts a changed Member into the list; a change to oneself also changes who is signed in. */
  async function changed(entry: MemberEntry) {
    members = members?.map(known => (known.id === entry.id ? entry : known)) ?? null
    if (entry.id === session.member?.id) await session.load()
  }

  function setAdmin(member: MemberEntry, workspace_admin: boolean) {
    return attempt(async () => changed(await call('set_workspace_admin', { id: member.id, workspace_admin })))
  }

  async function deactivate(member: MemberEntry) {
    const sure = await ask({
      title: `Deactivate ${member.name}?`,
      message: 'They are signed out at once and can no longer sign in.',
      action: 'Deactivate',
      destructive: true,
    })
    if (!sure) return
    return attempt(async () => changed(await call('deactivate_member', { id: member.id })))
  }

  async function removePicture(member: MemberEntry) {
    const sure = await ask({
      title: `Remove ${member.name}’s picture?`,
      message: 'Their initials are shown instead. Only they can choose a new picture.',
      action: 'Remove',
      destructive: true,
    })
    if (!sure) return
    return attempt(async () => {
      await call('remove_profile_picture', { id: member.id })
      await changed({ ...member, picture: null })
    })
  }

  function resetPassword(member: MemberEntry) {
    return attempt(async () => show(`Password reset for ${member.name}`, await call('create_password_reset', { id: member.id })))
  }

  const address = $derived(shown?.link.address ? location.origin + shown.link.address : '')
  const expires = $derived(shown ? new Date(shown.link.expires_at * 1000).toLocaleString() : '')

  async function copy() {
    await navigator.clipboard.writeText(address)
    copied = true
  }
</script>

<main class="screen">
  <h1>Members</h1>
  {#if loadError}
    <p class="muted section">{loadError}</p>
  {:else if members !== null}
    <form class="section stack" onsubmit={invite}>
      <h2 class="section-title">Invite a person</h2>
      <p class="muted">
        {session.sendsMail
          ? 'Turtle mails them a link. It works once and expires after seven days.'
          : 'You get a link to pass on. It works once and expires after seven days.'}
      </p>
      <div class="controls">
        <input class="input grow" type="email" aria-label="Email address" placeholder="Email address" required autocomplete="off" bind:value={email} />
        <button type="submit" class="btn primary" disabled={busy}>Invite</button>
      </div>
    </form>

    {#if shown}
      <section class="section notice" aria-live="polite">
        <strong>{shown.purpose}</strong>
        {#if shown.link.mailed_to}
          <p>Turtle has mailed the link to {shown.link.mailed_to}.</p>
        {:else}
          {#if shown.link.mail_failed_to}
            <p>Turtle could not mail the link to {shown.link.mail_failed_to}. Pass it on yourself.</p>
          {/if}
          <div class="controls">
            <input class="input grow" readonly aria-label="Link to pass on" value={address} onfocus={event => event.currentTarget.select()} />
            <button type="button" class="btn" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
          </div>
        {/if}
        <small class="hint">Works once, until {expires}.</small>
      </section>
    {/if}

    {#if error}
      <p class="error section" role="alert">{error}</p>
    {/if}

    <ul class="list section">
      {#each members as member (member.id)}
        <li class:inactive={!member.active}>
          <div class="item-person">
            <ProfilePicture name={member.name} picture={member.picture} />
            <div class="item">
              <span class="item-name">{member.name}</span>
              <small class="hint">{[member.email, member.workspace_admin && 'Workspace Admin', !member.active && 'Deactivated'].filter(Boolean).join(' · ')}</small>
            </div>
          </div>
          {#if member.active}
            <div class="controls">
              {#if member.workspace_admin}
                <button type="button" class="btn" disabled={busy} onclick={() => setAdmin(member, false)}>Remove Workspace Admin</button>
              {:else}
                <button type="button" class="btn" disabled={busy} onclick={() => setAdmin(member, true)}>Make Workspace Admin</button>
              {/if}
              {#if member.picture}
                <button type="button" class="btn" disabled={busy} onclick={() => removePicture(member)}>Remove picture</button>
              {/if}
              <button type="button" class="btn" disabled={busy} onclick={() => resetPassword(member)}>Password reset link</button>
              <button type="button" class="btn" disabled={busy} onclick={() => deactivate(member)}>Deactivate</button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</main>
