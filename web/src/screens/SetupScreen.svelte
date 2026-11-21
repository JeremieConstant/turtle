<script lang="ts">
  import AccountForm from '../components/AccountForm.svelte'
  import Field from '../components/Field.svelte'
  import { call } from '../lib/ops'
  import { router } from '../lib/router.svelte'
  import { session } from '../lib/session.svelte'

  let name = $state('')
  let email = $state('')
  let password = $state('')

  $effect(() => {
    document.title = 'Set up · Turtle'
  })

  async function createAccount() {
    session.signedIn(await call('create_first_account', { name, email, password }))
    router.navigate('/')
  }
</script>

<AccountForm
  heading="Welcome to Turtle"
  intro="Create the first account. It becomes the Workspace Admin, who invites everyone else."
  submitLabel="Create account"
  onsubmit={createAccount}
>
  <Field label="Name" autocomplete="name" bind:value={name} />
  <Field label="Email address" type="email" autocomplete="email" bind:value={email} />
  <Field
    label="Password"
    type="password"
    autocomplete="new-password"
    hint="At least 10 characters."
    bind:value={password}
  />
</AccountForm>
