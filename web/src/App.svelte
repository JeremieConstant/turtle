<script lang="ts">
  import { router } from './lib/router.svelte'
  import { pageAddress } from './lib/routes'
  import NotFoundScreen from './screens/NotFoundScreen.svelte'
  import PageScreen from './screens/PageScreen.svelte'

  const route = $derived(router.route)

  $effect(() => {
    // Until there is a list of Areas to land on, the start address opens the demo Page.
    if (route.screen === 'home') router.replace(pageAddress('demo', 'demo'))
  })
</script>

{#if route.screen === 'page'}
  {#key route.id}
    <PageScreen id={route.id} />
  {/key}
{:else if route.screen === 'not-found'}
  <NotFoundScreen />
{/if}
