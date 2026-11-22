/*
 * Attachments: images and files that belong to a Page. They are sent to
 * Turtle in base64 through attach_file, and fetched back from their `url`.
 */
import { call } from './ops'
import type { Attachment } from './operations.gen'

/** "1.5 KB", "25 MB": a size as people read it. */
export function fileSize(bytes: number): string {
  const KB = 1024
  const MB = KB * 1024
  if (bytes < KB) return bytes === 1 ? '1 byte' : `${bytes} bytes`
  const [amount, unit] = bytes < MB ? [bytes / KB, 'KB'] : [bytes / MB, 'MB']
  return `${Number(amount.toFixed(1))} ${unit}`
}

/** The content in base64. Encoded in slices, since one call can only take so many arguments. */
export function base64Of(content: Uint8Array): string {
  const SLICE = 0x8000
  let binary = ''
  for (let at = 0; at < content.length; at += SLICE) {
    binary += String.fromCharCode(...content.subarray(at, at + SLICE))
  }
  return btoa(binary)
}

/** Attaches a file to a Page. */
export async function attach(pageId: string, file: File): Promise<Attachment> {
  const content = base64Of(new Uint8Array(await file.arrayBuffer()))
  return call('attach_file', { page_id: pageId, name: file.name || `image.${file.type.split('/')[1] ?? 'png'}`, content })
}
