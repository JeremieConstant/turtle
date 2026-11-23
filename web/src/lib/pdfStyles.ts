/*
 * PDF Styles as the settings and the PDF Export dialog use them.
 */
import type { Input } from './ops'
import type { PdfStyle } from './operations.gen'

/** What a PDF Style is called and how a PDF made with it looks, as its form edits it. */
export type PdfStyleValues = Input<'create_pdf_style'>

/** The ID of the default PDF Style, the one an export uses unless another is chosen. */
export function defaultPdfStyleId(styles: PdfStyle[]): string | undefined {
  return styles.find(style => style.is_default)?.id
}

/** The values of a PDF Style, to edit it. */
export function valuesOf(style: PdfStyle): PdfStyleValues {
  const { id: _id, is_default: _isDefault, ...values } = style
  return { ...values, footer_text: values.footer_text ?? '' }
}

/** What a new PDF Style starts with: the default one's values, without a name. */
export function newPdfStyleValues(styles: PdfStyle[]): PdfStyleValues {
  const standard = styles.find(style => style.is_default) ?? styles[0]
  return { ...valuesOf(standard), name: '' }
}
