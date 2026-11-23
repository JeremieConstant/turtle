/*
 * Exporting an Area or a Subtree: the operation returns the ZIP in base64,
 * which is saved here as a download.
 */
import { call, type Input } from './ops'
import { reportFailure } from './toasts'

/**
 * Exports an Area, or the Subtree of an Entry in it, and saves the ZIP. A
 * failure is reported in a toast that names `title`, what was exported.
 */
export async function exportToFile(
  input: Input<'export'>,
  title: string,
  save: (name: string, file: Blob) => void = download,
) {
  try {
    const { file_name, content } = await call('export', input)
    const bytes = Uint8Array.from(atob(content), c => c.charCodeAt(0))
    save(file_name, new Blob([bytes], { type: 'application/zip' }))
  } catch (caught) {
    reportFailure(caught, `${title} could not be exported. Try again.`)
  }
}

/** Hands a file to the browser to save, as a link to it would. */
function download(name: string, file: Blob) {
  const link = document.createElement('a')
  link.href = URL.createObjectURL(file)
  link.download = name
  link.click()
  // The browser has taken the file once the click is handled.
  setTimeout(() => URL.revokeObjectURL(link.href))
}
