<script lang="ts">
  /*
   * PROTOTYPE (issue #14), not production. Three variants of the Trash on
   * /prototype/trash, switchable via ?variant=A|B|C and the floating bar.
   * ?area= names the Area for the variants that belong to one.
   */
  import { ui } from '../../lib/ui.svelte'
  import { variantOf } from './state.svelte'
  import VariantA from './VariantA.svelte'
  import VariantB from './VariantB.svelte'
  import VariantC from './VariantC.svelte'

  const { variant, area }: { variant: string | null; area: string | null } = $props()
  const key = $derived(variantOf(variant))
  const areaId = $derived(area ?? ui.openArea)
</script>

{#if key === 'A'}
  <VariantA {areaId} />
{:else if key === 'B'}
  <VariantB />
{:else}
  <VariantC {areaId} />
{/if}
