<script lang="ts">
  /*
   * An Area's mark: a rounded square with its first letter in the Area's
   * colour, so an Area is recognised at a glance wherever it shows. The
   * Personal Area has a person instead, being the one that is yours alone.
   */
  import { areaColor, areaInitial } from '../lib/areaMark'
  import Icon from './Icon.svelte'

  const { area, large = false }: { area: { id: string; name: string; personal: boolean }; large?: boolean } = $props()
  const color = $derived(areaColor(area.id))
</script>

<span
  class="mark"
  class:large
  class:personal={area.personal}
  style:--fill="var(--color-area-{color})"
  style:--letter="var(--color-area-{color}-text)"
  aria-hidden="true"
>
  {#if area.personal}
    <Icon name="person" size={large ? 18 : 12} />
  {:else}
    {areaInitial(area.name)}
  {/if}
</span>

<style>
  .mark {
    flex: none;
    display: inline-grid;
    place-items: center;
    width: var(--size-area-mark);
    height: var(--size-area-mark);
    border-radius: var(--radius-s);
    background: var(--fill);
    color: var(--letter);
    font-size: var(--text-area-mark);
    font-weight: var(--weight-strong);
    line-height: 1;
  }

  .mark.large {
    width: var(--size-area-mark-l);
    height: var(--size-area-mark-l);
    border-radius: var(--radius-m);
    font-size: var(--text-area-mark-l);
  }

  .mark.personal {
    background: var(--color-accent-soft);
    color: var(--color-accent-text);
  }
</style>
