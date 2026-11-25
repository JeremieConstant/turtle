<script lang="ts">
  import AccountForm from '../components/AccountForm.svelte'
  import Field from '../components/Field.svelte'
  import { call } from '../lib/ops'
  import { router } from '../lib/router.svelte'
  import { session } from '../lib/session.svelte'

  const { returnTo }: { returnTo: string } = $props()

  let email = $state('')
  let password = $state('')

  $effect(() => {
    document.title = 'Sign in · Turtle'
  })

  async function signIn() {
    session.signedIn(await call('sign_in', { email, password }))
    router.navigate(returnTo)
  }
</script>

<AccountForm heading="Sign in" intro="Sign in to your Workspace." submitLabel="Sign in" onsubmit={signIn}>
  <Field label="Email address" type="email" autocomplete="username" bind:value={email} />
  <Field label="Password" type="password" autocomplete="current-password" bind:value={password} />
  {#if session.sendsMail}
    <a class="hint" href="/forgot-password" onclick={event => router.follow(event, '/forgot-password')}>Forgot your password?</a>
  {/if}
</AccountForm>
