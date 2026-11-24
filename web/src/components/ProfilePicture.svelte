<script lang="ts">
  /*
   * A person's mark: their Profile picture in a circle, or their initials
   * without one. A picture that fails to load gives way to the initials too,
   * so every place that shows a person shows something.
   */
  import { initials } from '../lib/profile'

  const {
    name,
    size = 32,
    picture = null,
  }: {
    name: string
    size?: number
    /** The address of their Profile picture, if they have one. */
    picture?: string | null
  } = $props()

  // The address that failed, so a new picture is tried again.
  let failed = $state<string | null>(null)
</script>

<span class="profile-picture" style:--size="{size}px" aria-hidden="true">
  {#if picture && failed !== picture}
    <img src={picture} alt="" onerror={() => (failed = picture)} />
  {:else}
    {initials(name)}
  {/if}
</span>

<style>
  .profile-picture {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: var(--size);
    height: var(--size);
    overflow: hidden;
    border-radius: var(--radius-round);
    background: var(--color-sunken);
    color: var(--color-muted);
    font-size: calc(var(--size) * 0.4);
    font-weight: 600;
    line-height: 1;
    user-select: none;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
