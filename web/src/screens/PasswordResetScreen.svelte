<script lang="ts">
  import AccountForm from '../components/AccountForm.svelte'
  import Field from '../components/Field.svelte'
  import { call } from '../lib/ops'
  import { router } from '../lib/router.svelte'
  import { session } from '../lib/session.svelte'

  const { secret }: { secret: string } = $props()

  let password = $state('')

  $effect(() => {
    document.title = 'New password · Turtle'
  })

  async function reset() {
    session.signedIn(await call('reset_password', { secret, password }))
    router.navigate('/')
  }
</script>

<AccountForm
  heading="Choose a new password"
  intro="Setting a new password signs you out everywhere else."
  submitLabel="Set password"
  onsubmit={reset}
>
  <Field label="New password" type="password" autocomplete="new-password" hint="At least 10 characters." bind:value={password} />
</AccountForm>
