<script lang="ts">
  /*
   * The fonts PDF Styles may use, on the PDF Styles screen: each family with
   * its styles, those built into Turtle marked as such. A Workspace Admin
   * uploads TTF and OTF files here and deletes uploaded ones.
   */
  import { ask } from '../lib/confirm.svelte'
  import { uploadFonts } from '../lib/fonts'
  import { call, OperationError } from '../lib/ops'
  import type { FontFamily, FontStyle } from '../lib/operations.gen'

  const {
    onchange,
  }: {
    /** Called after a font was uploaded or deleted, as the families a PDF Style may use change. */
    onchange: () => void
  } = $props()

  let families = $state.raw<FontFamily[] | null>(null)
  let busy = $state(false)
  let errors = $state<string[]>([])
  let picker = $state<HTMLInputElement>()

  load().catch(() => (errors = ['The fonts could not be loaded. Reload the page to try again.']))

  async function load() {
    families = (await call('list_fonts', {})).families
  }

  async function chosen() {
    const files = [...(picker?.files ?? [])]
    // Cleared, so choosing the same file again uploads it again.
    if (picker) picker.value = ''
    if (!files.length) return
    busy = true
    errors = []
    try {
      errors = await uploadFonts(files)
      await load()
      onchange()
    } finally {
      busy = false
    }
  }

  async function remove(family: FontFamily, style: FontStyle) {
    const fontId = style.font_id
    if (!fontId) return
    const sure = await ask({
      title: `Delete ${family.name} ${style.style}?`,
      message: 'PDFs exported from now on cannot use this file.',
      action: 'Delete',
      destructive: true,
    })
    if (!sure) return
    busy = true
    errors = []
    try {
      await call('delete_font', { font_id: fontId })
      await load()
      onchange()
    } catch (caught) {
      errors = [caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.']
    } finally {
      busy = false
    }
  }
</script>

<section class="section stack" aria-labelledby="fonts-title">
  <div class="controls spread">
    <h2 class="section-title" id="fonts-title">Fonts</h2>
    <button type="button" class="btn" disabled={busy} onclick={() => picker?.click()}>Upload fonts</button>
    <input class="visually-hidden" type="file" accept=".ttf,.otf" multiple tabindex="-1" bind:this={picker} onchange={chosen} />
  </div>
  <p class="hint">
    Upload a font as TTF or OTF files of at most 10 MB, one for each style: regular, bold, italic and bold italic. Variable fonts, WOFF and
    WOFF2 cannot be used.
  </p>
  {#each errors as error (error)}
    <p class="error" role="alert">{error}</p>
  {/each}
  {#if families}
    <ul class="list">
      {#each families as family (family.name)}
        <li>
          <div class="item">
            <span class="item-name">{family.name}</span>
            {#if family.bundled}<small class="hint">Built into Turtle</small>{/if}
          </div>
          {#if family.bundled}
            <span class="hint">{family.styles.map(style => style.style).join(', ')}</span>
          {:else}
            <ul class="styles">
              {#each family.styles as style (style.font_id)}
                <li class="controls">
                  <span class="grow">{style.style} <small class="hint">{style.file_name}</small></span>
                  <button type="button" class="btn" disabled={busy} onclick={() => remove(family, style)}>Delete</button>
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .styles {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-2);
  }
</style>
