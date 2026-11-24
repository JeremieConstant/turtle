<script lang="ts">
  /*
   * A person's mark: their initials in a circle. A Profile picture, once
   * people can have one, takes the place of the initials here, so every
   * place that shows a person gets it at once.
   */
  import { initials } from '../lib/profile'

  const { name, size = 32 }: { name: string; size?: number } = $props()
</script>

<span class="avatar" style:--size="{size}px" aria-hidden="true">{initials(name)}</span>

<style>
  .avatar {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: var(--size);
    height: var(--size);
    border-radius: var(--radius-round);
    background: var(--color-sunken);
    color: var(--color-muted);
    font-size: calc(var(--size) * 0.4);
    font-weight: 600;
    line-height: 1;
    user-select: none;
  }
</style>
