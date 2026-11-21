<script lang="ts">
  import AccountForm from '../components/AccountForm.svelte'
  import Field from '../components/Field.svelte'
  import { call } from '../lib/ops'
  import { router } from '../lib/router.svelte'
  import { session } from '../lib/session.svelte'

  const { secret }: { secret: string } = $props()

  let name = $state('')
  let password = $state('')

  $effect(() => {
    document.title = 'Join · Turtle'
  })

  async function join() {
    session.signedIn(await call('accept_invitation', { secret, name, password }))
    router.navigate('/')
  }
</script>

<AccountForm
  heading="Join the Workspace"
  intro="You have been invited to Turtle. Choose your name and a password to create your account."
  submitLabel="Create account"
  onsubmit={join}
>
  <Field label="Name" autocomplete="name" bind:value={name} />
  <Field label="Password" type="password" autocomplete="new-password" hint="At least 10 characters." bind:value={password} />
</AccountForm>
