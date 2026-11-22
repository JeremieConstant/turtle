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

<label class="field">
  <span>{label}</span>
  <input class="input" {type} {autocomplete} required bind:value />
  {#if hint}<small class="hint">{hint}</small>{/if}
</label>
