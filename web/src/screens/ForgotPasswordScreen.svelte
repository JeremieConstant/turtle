<script lang="ts">
  import AccountForm from '../components/AccountForm.svelte'
  import Field from '../components/Field.svelte'
  import Logo from '../components/Logo.svelte'
  import { call } from '../lib/ops'
  import { router } from '../lib/router.svelte'

  let email = $state('')
  /** The address a link was asked for, once it was. */
  let askedFor = $state<string | null>(null)

  $effect(() => {
    document.title = 'Forgot password · Turtle'
  })

  async function ask() {
    await call('request_password_reset', { email })
    askedFor = email.trim()
  }
</script>

{#if askedFor === null}
  <AccountForm
    heading="Forgot your password?"
    intro="Enter the email address of your account. Turtle mails you a link to choose a new password."
    submitLabel="Send link"
    onsubmit={ask}
  >
    <Field label="Email address" type="email" autocomplete="username" bind:value={email} />
    <a class="hint" href="/sign-in" onclick={event => router.follow(event, '/sign-in')}>Back to sign in</a>
  </AccountForm>
{:else}
  <main class="standalone">
    <div class="standalone-card">
      <div>
        <strong class="wordmark"><Logo size="var(--size-mark)" />Turtle</strong>
        <h1>Check your mail</h1>
        <p class="muted" role="status">
          If {askedFor} belongs to an account, a link to choose a new password is on its way. It works once, within 24 hours.
        </p>
      </div>
      <a class="btn" href="/sign-in" onclick={event => router.follow(event, '/sign-in')}>Back to sign in</a>
    </div>
  </main>
{/if}
