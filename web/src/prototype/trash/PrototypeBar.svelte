<script lang="ts">
  /*
   * PROTOTYPE (issue #14). The floating bar that flips between the variants.
   * It sits on every screen of the shell, because two of the variants put
   * their way in into the sidebar, and that has to be judged from a real Page.
   */
  import { router } from '../../lib/router.svelte'
  import { proto, prototyping, reset, trash, VARIANTS, variantOf } from './state.svelte'

  const route = $derived(router.route)
  const onRoute = $derived(route.screen === 'prototype-trash')

  // The address is the truth while the prototype is on screen.
  $effect(() => {
    if (route.screen === 'prototype-trash') proto.variant = variantOf(route.variant)
  })

  const current = $derived(VARIANTS.find(v => v.key === proto.variant) ?? VARIANTS[0])

  function show(step: number) {
    const index = VARIANTS.findIndex(v => v.key === proto.variant)
    const next = VARIANTS[(index + step + VARIANTS.length) % VARIANTS.length]
    proto.variant = next.key
    if (!onRoute) return
    const query = new URLSearchParams(location.search)
    query.set('variant', next.key)
    router.rewrite(`/prototype/trash?${query}`)
  }

  function onkeydown(event: KeyboardEvent) {
    if (!onRoute || event.metaKey || event.ctrlKey || event.altKey) return
    if (event.target instanceof Element && event.target.closest('input, textarea, select, [contenteditable]')) return
    if (event.key === 'ArrowLeft') show(-1)
    else if (event.key === 'ArrowRight') show(1)
  }
</script>

<svelte:window {onkeydown} />

{#if prototyping}
  <div class="bar" role="group" aria-label="Prototype: Trash">
    <button type="button" aria-label="Previous variant" onclick={() => show(-1)}>←</button>
    <span class="label">
      <b>{current.key}</b>
      {current.name}
    </span>
    <button type="button" aria-label="Next variant" onclick={() => show(1)}>→</button>
    <span class="rule"></span>
    {#if onRoute}
      <span class="state">{trash.items.length} in the Trash</span>
      <button type="button" class="text" onclick={reset}>Reset</button>
    {:else}
      <a class="text" href="/prototype/trash?variant={current.key}" onclick={event => router.follow(event, `/prototype/trash?variant=${current.key}`)}>
        Open the Trash prototype
      </a>
    {/if}
  </div>
{/if}

<style>
  /* Deliberately unlike the app: dark, round, floating. It is not part of the design. */
  .bar {
    position: fixed;
    left: 50%;
    bottom: var(--space-4);
    z-index: var(--layer-drag);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    max-width: var(--width-floating-max);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-round);
    background: var(--color-text);
    color: var(--color-bg);
    font-size: var(--text-s);
    box-shadow: var(--shadow-float);
    transform: translateX(-50%);
    white-space: nowrap;
  }

  button,
  .text {
    height: var(--size-icon-button);
    min-width: var(--size-icon-button);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-round);
    background: none;
    color: inherit;
    font-size: inherit;
    line-height: var(--size-icon-button);
    text-decoration: none;
  }

  button:hover,
  .text:hover {
    background: var(--color-text-2);
  }

  .label b {
    margin-right: var(--space-1);
  }

  .rule {
    width: var(--line);
    height: var(--space-4);
    background: var(--color-muted);
  }

  .state {
    color: var(--color-faint);
  }
</style>
