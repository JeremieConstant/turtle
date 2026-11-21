<script lang="ts" module>
  /** A button of the form: what it says, and what is passed on when it is pressed. */
  export type Action<V extends string = string> = { label: string; value: V }

  /** Adding to the tree: as a Page or as a Folder. */
  export const ADD_ACTIONS: Action<'page' | 'folder'>[] = [
    { label: 'Add Page', value: 'page' },
    { label: 'Add Folder', value: 'folder' },
  ]
</script>

<script lang="ts" generics="V extends string">
  import { onMount } from 'svelte'
  import { OperationError } from '../lib/ops'

  /**
   * One line to enter a title and act on it: creating a Page or Folder, or
   * renaming one. Each action is a submit button; the one pressed is passed on.
   */
  const {
    label,
    initial = '',
    actions,
    onsubmit,
    oncancel,
  }: {
    /** What the field is for, read out by screen readers. */
    label: string
    initial?: string
    /** The buttons; the first is pressed by Enter. */
    actions: Action<V>[]
    onsubmit: (title: string, action: V) => Promise<void>
    oncancel?: () => void
  } = $props()

  let input = $state<HTMLInputElement | null>(null)
  let title = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)

  onMount(() => {
    title = initial
    // A given title is selected, so typing replaces it.
    queueMicrotask(() => input?.select())
  })

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    const pressed = actions.find(
      action => event.submitter instanceof HTMLButtonElement && action.value === event.submitter.value,
    )
    busy = true
    error = null
    try {
      await onsubmit(title, (pressed ?? actions[0]).value)
      title = ''
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && oncancel) oncancel()
  }
</script>

<form onsubmit={submit}>
  <div class="row">
    <input
      aria-label={label}
      placeholder="Title"
      required
      autocomplete="off"
      bind:this={input}
      bind:value={title}
      onkeydown={keydown}
    />
    {#each actions as action (action.value)}
      <button type="submit" value={action.value} disabled={busy}>{action.label}</button>
    {/each}
    {#if oncancel}
      <button type="button" onclick={oncancel}>Cancel</button>
    {/if}
  </div>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
</form>

<style>
  form {
    margin: var(--space-1) 0;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  input {
    flex: 1;
    min-width: 0;
  }

  .error {
    margin: var(--space-1) 0 0;
    color: var(--color-danger);
  }
</style>
