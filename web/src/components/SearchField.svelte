<script lang="ts">
  /* The search field of a screen that is about searching: it has the focus as soon as the screen is there. */
  import { onMount } from 'svelte'
  import type { HTMLInputAttributes } from 'svelte/elements'
  import Icon from './Icon.svelte'

  let {
    value = $bindable(),
    ...rest
  }: {
    value: string
  } & HTMLInputAttributes = $props()

  let field: HTMLInputElement
  onMount(() => field.focus())
</script>

<form class="field" role="search" onsubmit={event => event.preventDefault()}>
  <Icon name="search" size={18} />
  <input
    type="search"
    placeholder="Search every Area"
    aria-label="Search every Area"
    autocomplete="off"
    bind:this={field}
    bind:value
    {...rest}
  />
</form>

<style>
  .field {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--height-search);
    padding: 0 var(--space-4);
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-l);
    background: var(--color-surface);
    color: var(--color-muted);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) var(--ease-out);
  }

  .field:focus-within {
    border-color: var(--color-focus);
    box-shadow: 0 0 0 var(--ring) var(--color-focus-soft);
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--color-text);
    font-size: var(--text-l);
  }

  input::placeholder {
    color: var(--color-muted);
  }

  /* Escape clears the field, so the browser's own button would be a second way to say the same. */
  input::-webkit-search-cancel-button {
    display: none;
  }
</style>
