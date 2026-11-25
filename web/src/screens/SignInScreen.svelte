<script lang="ts">
  import AccountForm from '../components/AccountForm.svelte'
  import DemoSignIn from '../components/DemoSignIn.svelte'
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

  const demoAccount = $derived(session.demo?.account ?? null)

  async function signIn(credentials: { email: string; password: string }) {
    session.signedIn(await call('sign_in', credentials))
    router.navigate(returnTo)
  }
</script>

<AccountForm
  heading="Sign in"
  intro={demoAccount ? 'Open the demo, or sign in with another account.' : 'Sign in to your Workspace.'}
  submitLabel="Sign in"
  onsubmit={() => signIn({ email, password })}
  quietSubmit={demoAccount !== null}
>
  {#snippet lead()}
    {#if demoAccount}
      <DemoSignIn account={demoAccount} onopen={() => signIn(demoAccount)} />
    {/if}
  {/snippet}
  <Field label="Email address" type="email" autocomplete="username" bind:value={email} />
  <Field label="Password" type="password" autocomplete="current-password" bind:value={password} />
  {#if session.sendsMail}
    <a class="hint" href="/forgot-password" onclick={event => router.follow(event, '/forgot-password')}>Forgot your password?</a>
  {/if}
</AccountForm>
