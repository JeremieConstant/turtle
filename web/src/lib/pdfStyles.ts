/*
 * PDF Styles as the settings and the PDF Export dialog use them.
 */
import { base64Of } from './attachments'
import { call, type Input } from './ops'
import type { PdfStyle } from './operations.gen'

/** What a PDF Style is called and how a PDF made with it looks, as its form edits it. */
export type PdfStyleValues = Input<'create_pdf_style'>

/** What becomes of a PDF Style's logo when its form is saved: a new file, removed, or nothing. */
export type LogoChange = { file: File } | 'remove' | null

/** The ID of the default PDF Style, the one an export uses unless another is chosen. */
export function defaultPdfStyleId(styles: PdfStyle[]): string | undefined {
  return styles.find(style => style.is_default)?.id
}

/** The values of a PDF Style, to edit it. Its logo is changed on its own. */
export function valuesOf(style: PdfStyle): PdfStyleValues {
  const { id: _id, is_default: _isDefault, logo: _logo, ...values } = style
  return { ...values, footer_text: values.footer_text ?? '' }
}

/** What a new PDF Style starts with: the default one's values, without a name. */
export function newPdfStyleValues(styles: PdfStyle[]): PdfStyleValues {
  const standard = styles.find(style => style.is_default) ?? styles[0]
  return { ...valuesOf(standard), name: '' }
}

/** Gives a PDF Style the chosen logo, or removes its logo. */
export async function changeLogo(pdfStyleId: string, change: LogoChange): Promise<void> {
  if (change === 'remove') {
    await call('remove_pdf_style_logo', { pdf_style_id: pdfStyleId })
  } else if (change) {
    const content = base64Of(new Uint8Array(await change.file.arrayBuffer()))
    await call('set_pdf_style_logo', { pdf_style_id: pdfStyleId, file_name: change.file.name, content })
  }
}
