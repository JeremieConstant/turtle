/*
 * Fonts uploaded for PDF Styles.
 */
import { base64Of } from './attachments'
import { call, OperationError } from './ops'

/**
 * Uploads the files one after another, each one style of a font family.
 * Returns why each refused file was refused, naming it; empty when all were taken.
 */
export async function uploadFonts(files: File[]): Promise<string[]> {
  const refused: string[] = []
  for (const file of files) {
    try {
      const content = base64Of(new Uint8Array(await file.arrayBuffer()))
      await call('upload_font', { file_name: file.name, content })
    } catch (caught) {
      refused.push(
        caught instanceof OperationError ? `${file.name}: ${caught.message}` : `${file.name} could not be uploaded. Try again.`,
      )
    }
  }
  return refused
}
