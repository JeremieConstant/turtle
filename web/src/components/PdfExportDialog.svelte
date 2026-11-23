<script lang="ts">
  /*
   * Asks for what a PDF Export needs besides what to export, the PDF Style
   * and the subtitle on its cover, then exports it and saves the PDF.
   */
  import { tick } from 'svelte'
  import { exportPdfToFile } from '../lib/exporting'
  import { call, OperationError } from '../lib/ops'
  import type { PdfStyle } from '../lib/operations.gen'
  import { defaultPdfStyleId } from '../lib/pdfStyles'

  let dialog: HTMLDialogElement
  let input: HTMLInputElement
  let target = $state<{ area_id: string; entry_id?: string; title: string } | null>(null)
  let subtitle = $state('')
  let styles = $state.raw<PdfStyle[]>([])
  let styleId = $state<string | undefined>(undefined)
  let busy = $state(false)
  let error = $state<string | null>(null)

  /** Opens the dialog for an Area, or for the Subtree of an Entry in it; `title` names what is exported. */
  export async function open(area_id: string, entry_id: string | undefined, title: string) {
    target = { area_id, entry_id, title }
    subtitle = ''
    error = null
    dialog.showModal()
    await tick()
    input.focus()
    try {
      styles = (await call('list_pdf_styles', {})).pdf_styles
      styleId = defaultPdfStyleId(styles)
    } catch {
      // Without the list, the PDF is made with the default PDF Style.
      styles = []
      styleId = undefined
    }
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    if (!target) return
    busy = true
    error = null
    try {
      const { area_id, entry_id } = target
      const shown = subtitle.trim()
      await exportPdfToFile({
        area_id,
        ...(entry_id ? { entry_id } : {}),
        ...(shown ? { subtitle: shown } : {}),
        ...(styleId ? { pdf_style_id: styleId } : {}),
      })
      dialog.close()
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }
</script>

<dialog bind:this={dialog} closedby="any" aria-label={target ? `Export ${target.title} as PDF` : 'Export as PDF'}>
  <form class="dialog-body" onsubmit={submit}>
    <p class="heading">Export {target?.title} as PDF</p>
    <label>
      <span>Subtitle on the cover <span class="muted">(optional)</span></span>
      <input bind:this={input} bind:value={subtitle} autocomplete="off" />
    </label>
    {#if styles.length > 0}
      <label>
        <span>PDF Style</span>
        <select bind:value={styleId}>
          {#each styles as style (style.id)}
            <option value={style.id}>{style.is_default ? `${style.name} (default)` : style.name}</option>
          {/each}
        </select>
      </label>
    {/if}
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
    <div class="dialog-actions">
      <button type="button" class="btn" onclick={() => dialog.close()}>Cancel</button>
      <button type="submit" class="btn primary" disabled={busy}>{busy ? 'Exporting…' : 'Export'}</button>
    </div>
  </form>
</dialog>

<style>
  .heading {
    margin: 0;
    font-weight: var(--weight-strong);
  }

  label {
    display: grid;
    gap: var(--space-2);
    font-size: var(--text-s);
    font-weight: var(--weight-medium);
  }

  input,
  select {
    height: var(--space-8);
    font-size: var(--text-base);
  }

  input:focus,
  select:focus {
    outline: none;
    border-color: var(--color-focus);
    box-shadow: 0 0 0 var(--ring) var(--color-focus-soft);
  }

  .muted {
    font-weight: normal;
  }

  .error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--text-m);
  }
</style>
