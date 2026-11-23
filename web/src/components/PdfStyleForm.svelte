<script lang="ts">
  /*
   * The values of a PDF Style, to create or edit one. Each group of values
   * is a fieldset of its own.
   */
  import type { PdfStyleLogo } from '../lib/operations.gen'
  import type { LogoChange, PdfStyleValues } from '../lib/pdfStyles'

  let {
    values,
    logo = null,
    fontFamilies,
    action,
    busy,
    onsave,
    oncancel,
  }: {
    /** What the form starts with. */
    values: PdfStyleValues
    /** The logo the PDF Style has so far. */
    logo?: PdfStyleLogo | null
    /** The font families a PDF Style may use. */
    fontFamilies: string[]
    /** The words on the button that saves. */
    action: string
    busy: boolean
    onsave: (values: PdfStyleValues, logo: LogoChange) => void
    oncancel: () => void
  } = $props()

  // The form edits its own copy; the values it was given stay as they were.
  // svelte-ignore state_referenced_locally
  let edited = $state<PdfStyleValues>({ ...values })

  // The logo changes with the other values, when the form is saved.
  let logoChange = $state<LogoChange>(null)
  let logoInput = $state<HTMLInputElement>()
  /** A chosen file, shown before it is saved. */
  let chosenUrl = $state<string | null>(null)
  $effect(() => {
    if (logoChange === null || logoChange === 'remove') return
    const url = URL.createObjectURL(logoChange.file)
    chosenUrl = url
    return () => URL.revokeObjectURL(url)
  })
  const shownLogo = $derived(logoChange === 'remove' ? null : logoChange ? chosenUrl : (logo?.url ?? null))

  function chooseLogo(event: Event) {
    const file = (event.currentTarget as HTMLInputElement).files?.[0]
    logoChange = file ? { file } : null
  }

  function removeLogo() {
    logoChange = logo ? 'remove' : null
    if (logoInput) logoInput.value = ''
  }

  const colors = [
    ['text_color', 'Text'],
    ['heading_color', 'Headings'],
    ['accent_color', 'Accent', 'Links, rules, Callout bars and the numbers in the table of contents.'],
  ] as const
  const fonts = [
    ['body_font', 'Body text'],
    ['heading_font', 'Headings'],
    ['code_font', 'Code'],
  ] as const
  const sizes = [
    ['body_size', 'Body text', 8, 14],
    ['h1_size', 'Chapter titles', 10, 36],
    ['h2_size', 'Headings, level 2', 10, 36],
    ['h3_size', 'Headings, level 3', 10, 36],
  ] as const

  function submit(event: SubmitEvent) {
    event.preventDefault()
    const footer = (edited.footer_text ?? '').trim()
    onsave({ ...edited, footer_text: footer === '' ? null : footer }, logoChange)
  }
</script>

<form class="stack" onsubmit={submit}>
  <label class="field">
    <span>Name</span>
    <input class="input" required autocomplete="off" maxlength="200" bind:value={edited.name} />
  </label>

  <fieldset class="choices">
    <legend>Logo</legend>
    {#if shownLogo}
      <img class="logo" src={shownLogo} alt="The logo" />
    {:else}
      <p class="hint">No logo.</p>
    {/if}
    <label class="field">
      <span>{shownLogo ? 'Replace with' : 'Add'} a PNG, JPEG or SVG of at most 2 MB</span>
      <input bind:this={logoInput} class="input" type="file" accept="image/png,image/jpeg,image/svg+xml" onchange={chooseLogo} />
      <small class="hint">Shown on the cover and small in the footer of every page after it.</small>
    </label>
    {#if shownLogo}
      <span class="controls">
        <button type="button" class="btn" onclick={removeLogo}>Remove logo</button>
      </span>
    {/if}
  </fieldset>

  <fieldset class="choices">
    <legend>Colours</legend>
    <div class="row">
      {#each colors as [key, label, hint] (key)}
        <label class="field">
          <span>{label}</span>
          <span class="controls">
            <input type="color" class="swatch" aria-label="{label} colour" bind:value={edited[key]} />
            <input class="input hex" aria-label="{label} colour as #rrggbb" required pattern={'#[0-9a-fA-F]{6}'} bind:value={edited[key]} />
          </span>
          {#if hint}<small class="hint">{hint}</small>{/if}
        </label>
      {/each}
    </div>
  </fieldset>

  <fieldset class="choices">
    <legend>Fonts</legend>
    <div class="row">
      {#each fonts as [key, label] (key)}
        <label class="field">
          <span>{label}</span>
          <select class="input" bind:value={edited[key]}>
            {#each fontFamilies as family (family)}
              <option value={family}>{family}</option>
            {/each}
          </select>
        </label>
      {/each}
    </div>
  </fieldset>

  <fieldset class="choices">
    <legend>Sizes in points</legend>
    <div class="row">
      {#each sizes as [key, label, min, max] (key)}
        <label class="field">
          <span>{label}</span>
          <input class="input number" type="number" required {min} {max} step="0.1" bind:value={edited[key]} />
          <small class="hint">{min} to {max}</small>
        </label>
      {/each}
    </div>
  </fieldset>

  <fieldset class="choices">
    <legend>Structure</legend>
    <label class="check"><input type="checkbox" bind:checked={edited.cover} /> Cover with title and date</label>
    <label class="check"><input type="checkbox" bind:checked={edited.toc} /> Table of contents</label>
    <label class="field">
      <span>The table of contents lists</span>
      <select class="input" disabled={!edited.toc} bind:value={edited.toc_depth}>
        <option value={1}>Chapters</option>
        <option value={2}>Chapters and their headings</option>
        <option value={3}>Chapters and two levels of headings</option>
      </select>
    </label>
  </fieldset>

  <fieldset class="choices">
    <legend>Footer and language</legend>
    <label class="field">
      <span>Footer text <span class="hint">(optional, one line)</span></span>
      <input class="input" autocomplete="off" maxlength="200" bind:value={edited.footer_text} />
    </label>
    <label class="field">
      <span>Language of the PDF's own words and date</span>
      <select class="input" bind:value={edited.language}>
        <option value="en">English: Contents, As of 23 November 2026</option>
        <option value="de">German: Inhalt, Stand 23. November 2026</option>
      </select>
    </label>
  </fieldset>

  <div class="controls">
    <button type="submit" class="btn primary" disabled={busy}>{action}</button>
    <button type="button" class="btn" onclick={oncancel}>Cancel</button>
  </div>
</form>

<style>
  fieldset {
    justify-self: stretch;
  }

  .row {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--width-menu-s), 1fr));
    align-items: start;
    gap: var(--space-3);
  }

  .logo {
    max-width: calc(4 * var(--space-12));
    max-height: calc(2 * var(--space-12));
    object-fit: contain;
    object-position: left;
  }

  .swatch {
    width: var(--height-control);
    height: var(--height-control);
    padding: 0;
    border: var(--line) solid var(--color-line-strong);
    border-radius: var(--radius-m);
    background: none;
  }

  .hex {
    width: var(--width-text-style);
  }

  .number {
    width: calc(2 * var(--space-12));
  }
</style>
