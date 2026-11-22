<script lang="ts">
  import { ask } from '../lib/confirm.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { Comparison, Version, VersionSummary } from '../lib/operations.gen'
  import { authorNames, previousNumber, versionTitle } from '../lib/versions'

  const { pageId, editable }: { pageId: string; editable: boolean } = $props()

  let versions = $state.raw<VersionSummary[] | null>(null)
  let selected = $state<number | null>(null)
  /** The Version the selected one is compared with; without one, its content is shown. */
  let comparedWith = $state<number | null>(null)
  let version = $state.raw<Version | null>(null)
  let comparison = $state.raw<Comparison | null>(null)
  let problem = $state('')
  let restoring = $state(false)

  function select(number: number) {
    if (selected === number) {
      selected = null
      return
    }
    selected = number
    comparedWith = versions ? previousNumber(versions, number) : null
  }

  function failed(caught: unknown) {
    problem = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
  }

  async function load() {
    problem = ''
    try {
      versions = (await call('list_versions', { page_id: pageId })).versions
    } catch (caught) {
      failed(caught)
    }
  }
  void load()

  $effect(() => {
    const number = selected
    const from = comparedWith
    version = null
    comparison = null
    if (number === null) return
    let shown = true
    const request =
      from !== null
        ? call('compare_versions', { page_id: pageId, from, to: number }).then(found => {
            if (shown) comparison = found
          })
        : call('get_version', { page_id: pageId, number }).then(found => {
            if (shown) version = found
          })
    request.catch(failed)
    return () => {
      shown = false
    }
  })

  async function restore(number: number) {
    const sure = await ask({
      title: `Restore Version ${number}?`,
      message: `The Page says again what Version ${number} said. This is recorded as a new Version.`,
      action: 'Restore',
    })
    if (!sure) return
    restoring = true
    problem = ''
    try {
      await call('restore_version', { page_id: pageId, number })
      selected = null
      await load()
    } catch (caught) {
      failed(caught)
    } finally {
      restoring = false
    }
  }

  const when = (seconds: number) =>
    new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
</script>

<aside class="stack">
  <div class="controls spread">
    <h2 class="section-title">Versions</h2>
    <button type="button" class="btn" onclick={() => void load()}>Refresh</button>
  </div>
  {#if problem}
    <p class="error" role="alert">{problem}</p>
  {/if}
  {#if versions === null}
    <!-- Loading. -->
  {:else if versions.length === 0}
    <p class="hint">No Versions yet. One is recorded when the last editor leaves, every two minutes while editing goes on, and for each change an Agent makes.</p>
  {:else}
    <ol class="picker">
      {#each versions as listed (listed.number)}
        <li>
          <button
            type="button"
            class="picker-row"
            aria-pressed={selected === listed.number}
            onclick={() => select(listed.number)}
          >
            <span class="item-name">{versionTitle(listed)}</span>
            <span class="hint">{when(listed.recorded_at)} · {authorNames(listed.authors)}</span>
          </button>
          {#if selected === listed.number}
            <div class="stack">
              <div class="controls">
                {#if versions.length > 1}
                  <label class="controls hint">
                    Compare with
                    <select class="input" bind:value={comparedWith}>
                      <option value={null}>Nothing, show the content</option>
                      {#each versions.filter(other => other.number !== listed.number) as other (other.number)}
                        <option value={other.number}>Version {other.number}</option>
                      {/each}
                    </select>
                  </label>
                {/if}
                {#if editable}
                  <button type="button" class="btn" disabled={restoring} onclick={() => void restore(listed.number)}>Restore</button>
                {/if}
              </div>
              {#if comparison}
                <pre class="code-block">{#each comparison.lines as line}<span class={line.change}
                      >{line.change === 'added' ? '+ ' : line.change === 'removed' ? '− ' : '  '}{line.text}
</span>{/each}</pre>
              {:else if version}
                <pre class="code-block">{version.markdown}</pre>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}
</aside>

