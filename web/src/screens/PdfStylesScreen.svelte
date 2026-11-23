<script lang="ts">
  import PdfFonts from '../components/PdfFonts.svelte'
  import PdfStyleForm from '../components/PdfStyleForm.svelte'
  import { ask } from '../lib/confirm.svelte'
  import { call, OperationError } from '../lib/ops'
  import type { PdfStyle } from '../lib/operations.gen'
  import { changeLogo, newPdfStyleValues, valuesOf, type LogoChange, type PdfStyleValues } from '../lib/pdfStyles'
  import { session } from '../lib/session.svelte'

  let styles = $state.raw<PdfStyle[] | null>(null)
  let fontFamilies = $state.raw<string[]>([])
  let loadError = $state<string | null>(null)
  let busy = $state(false)
  let error = $state<string | null>(null)
  /** What the form is open for: a new PDF Style, or the ID of the one being edited. */
  let editing = $state<'new' | string | null>(null)

  if (!session.member?.workspace_admin) {
    loadError = 'Only a Workspace Admin can manage PDF Styles.'
  } else {
    load().catch(() => (loadError = 'The PDF Styles could not be loaded. Reload the page to try again.'))
  }

  $effect(() => {
    document.title = 'PDF Styles · Turtle'
  })

  async function load() {
    const list = await call('list_pdf_styles', {})
    styles = list.pdf_styles
    fontFamilies = list.font_families
  }

  /** Runs an operation, showing what went wrong if it fails, and loads the PDF Styles again after it. */
  async function attempt(run: () => Promise<void>) {
    busy = true
    error = null
    try {
      await run()
      await load()
    } catch (caught) {
      error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      busy = false
    }
  }

  function save(values: PdfStyleValues, logo: LogoChange) {
    const id = editing
    return attempt(async () => {
      if (!id) return
      const saved = id === 'new' ? await call('create_pdf_style', values) : await call('update_pdf_style', { ...values, id })
      // Created now; should the logo be refused, the form stays open on it.
      editing = saved.id
      await changeLogo(saved.id, logo)
      editing = null
    })
  }

  function open(target: 'new' | string) {
    error = null
    editing = target
  }

  async function remove(style: PdfStyle) {
    const sure = await ask({
      title: `Delete ${style.name}?`,
      message: 'PDFs exported from now on cannot use it.',
      action: 'Delete',
      destructive: true,
    })
    if (!sure) return
    return attempt(() => call('delete_pdf_style', { id: style.id }).then(() => undefined))
  }
</script>

<main class="screen">
  <h1>PDF Styles</h1>
  <p class="lede">
    A PDF Style sets how an exported PDF looks: its logo, colours, fonts and sizes, whether it has a cover and a table of contents, and the
    language of its own words. Whoever exports a PDF chooses one; the default is chosen for them.
  </p>
  {#if loadError}
    <p class="muted">{loadError}</p>
  {:else if styles !== null}
    {#if editing === 'new'}
      <section class="section">
        <h2 class="section-title">New PDF Style</h2>
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        <PdfStyleForm values={newPdfStyleValues(styles)} {fontFamilies} action="Create PDF Style" {busy} onsave={save} oncancel={() => (editing = null)} />
      </section>
    {:else}
      <div class="controls">
        <button type="button" class="btn primary" onclick={() => open('new')}>New PDF Style</button>
      </div>
      {#if error && editing === null}
        <p class="error section" role="alert">{error}</p>
      {/if}
    {/if}

    <ul class="list section">
      {#each styles as style (style.id)}
        <li>
          {#if editing === style.id}
            <div class="detail">
              {#if error}<p class="error" role="alert">{error}</p>{/if}
              <PdfStyleForm values={valuesOf(style)} logo={style.logo} {fontFamilies} action="Save" {busy} onsave={save} oncancel={() => (editing = null)} />
            </div>
          {:else}
            <div class="item">
              <span class="item-name">{style.name}</span>
              <small class="hint">{style.is_default ? 'Default' : ''}</small>
            </div>
            <div class="controls">
              {#if !style.is_default}
                <button type="button" class="btn" disabled={busy} onclick={() => attempt(() => call('set_default_pdf_style', { id: style.id }).then(() => undefined))}>
                  Set as default
                </button>
              {/if}
              <button type="button" class="btn" disabled={busy} onclick={() => open(style.id)}>Edit</button>
              {#if !style.is_default}
                <button type="button" class="btn" disabled={busy} onclick={() => remove(style)}>Delete</button>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>

    <PdfFonts onchange={() => load().catch(() => undefined)} />
  {/if}
</main>
