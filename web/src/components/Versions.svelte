<script lang="ts">
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
    if (!confirm(`Make this Page say what Version ${number} said? This is recorded as a new Version.`)) return
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

<aside>
  <div class="heading">
    <h2>Versions</h2>
    <button type="button" onclick={() => void load()}>Refresh</button>
  </div>
  {#if problem}
    <p class="problem" role="alert">{problem}</p>
  {/if}
  {#if versions === null}
    <!-- Loading. -->
  {:else if versions.length === 0}
    <p class="muted">No Versions yet. One is recorded when the last editor leaves, every two minutes while editing goes on, and for each change an Agent makes.</p>
  {:else}
    <ol>
      {#each versions as listed (listed.number)}
        <li>
          <button
            type="button"
            class="entry"
            aria-pressed={selected === listed.number}
            onclick={() => select(listed.number)}
          >
            <span class="title">{versionTitle(listed)}</span>
            <span class="muted">{when(listed.recorded_at)} · {authorNames(listed.authors)}</span>
          </button>
          {#if selected === listed.number}
            <div class="detail">
              <div class="actions">
                {#if versions.length > 1}
                  <label>
                    Compare with
                    <select bind:value={comparedWith}>
                      <option value={null}>Nothing, show the content</option>
                      {#each versions.filter(other => other.number !== listed.number) as other (other.number)}
                        <option value={other.number}>Version {other.number}</option>
                      {/each}
                    </select>
                  </label>
                {/if}
                {#if editable}
                  <button type="button" disabled={restoring} onclick={() => void restore(listed.number)}>Restore</button>
                {/if}
              </div>
              {#if comparison}
                <pre>{#each comparison.lines as line}<span class={line.change}
                      >{line.change === 'added' ? '+ ' : line.change === 'removed' ? '− ' : '  '}{line.text}
</span>{/each}</pre>
              {:else if version}
                <pre>{version.markdown}</pre>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}
</aside>

<style>
  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-2);
  }

  h2 {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-s);
    font-weight: var(--weight-strong);
    text-transform: uppercase;
    letter-spacing: var(--tracking-small);
  }

  ol {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .entry {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-l);
    background: var(--color-surface);
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .entry[aria-pressed='true'] {
    border-color: var(--color-focus);
  }

  .title {
    font-weight: var(--weight-strong);
  }

  .muted {
    color: var(--color-muted);
    font-size: var(--text-s);
  }

  .problem {
    color: var(--color-danger);
  }

  .detail {
    margin-top: var(--space-2);
  }

  label {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--text-s);
  }

  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  pre {
    margin: 0;
    padding: var(--space-3);
    border: var(--line) solid var(--color-line);
    border-radius: var(--radius-l);
    background: var(--color-surface);
    font: var(--text-s) / var(--leading-code) var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .added {
    color: var(--color-focus);
  }

  .removed {
    color: var(--color-danger);
    text-decoration: line-through;
  }
</style>
