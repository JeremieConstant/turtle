<script lang="ts">
  import type * as Y from 'yjs'

  const { pageId, doc }: { pageId: string; doc: Y.Doc } = $props()

  let text = $state('')

  $effect(() => {
    let shown = true
    let pending: ReturnType<typeof setTimeout> | undefined
    async function load() {
      const response = await fetch(`/api/pages/${encodeURIComponent(pageId)}/markdown`)
      const loaded = response.ok ? await response.text() : `Could not load Markdown (${response.status}).`
      if (shown) text = loaded
    }
    // Follows the editing, but waits for a pause instead of asking per keystroke.
    const reload = () => {
      clearTimeout(pending)
      pending = setTimeout(() => void load(), 400)
    }
    void load()
    doc.on('update', reload)
    return () => {
      shown = false
      clearTimeout(pending)
      doc.off('update', reload)
    }
  })
</script>

<aside>
  <h2 class="section-title heading">Markdown</h2>
  <pre>{text}</pre>
</aside>

<style>
  .heading {
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
</style>
