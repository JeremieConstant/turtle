<script lang="ts">
  let {
    label,
    value = $bindable(''),
    type = 'text',
    autocomplete,
    hint,
  }: {
    label: string
    value?: string
    type?: 'text' | 'email' | 'password'
    autocomplete: AutoFill
    hint?: string
  } = $props()
</script>

<label>
  <span>{label}</span>
  <input {type} {autocomplete} required bind:value />
  {#if hint}<small>{hint}</small>{/if}
</label>

<style>
  label {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  span {
    font-size: var(--text-m);
    font-weight: var(--weight-strong);
  }

  small {
    color: var(--color-muted);
    font-size: var(--text-s);
  }
</style>
