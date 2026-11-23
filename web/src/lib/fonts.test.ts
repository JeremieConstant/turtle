import { beforeEach, expect, test, vi } from 'vitest'
import { uploadFonts } from './fonts'
import { OperationError } from './ops'

const call = vi.hoisted(() => vi.fn())
vi.mock('./ops', async original => ({ ...(await original<typeof import('./ops')>()), call }))

beforeEach(() => vi.clearAllMocks())

test('each chosen file is uploaded in base64, and each refused one is named with the reason', async () => {
  call
    .mockResolvedValueOnce({ family: 'Corporate Sans', style: 'Bold' })
    .mockRejectedValueOnce(new OperationError('invalid_input', 'This is a WOFF or WOFF2 font, which PDFs cannot use. Upload the font as a TTF or OTF file.'))
  const bold = new File([new TextEncoder().encode('hello')], 'Corporate-Bold.ttf')
  const woff = new File([new Uint8Array([1])], 'Corporate.woff')

  const refused = await uploadFonts([bold, woff])

  expect(call).toHaveBeenNthCalledWith(1, 'upload_font', { file_name: 'Corporate-Bold.ttf', content: 'aGVsbG8=' })
  expect(call).toHaveBeenNthCalledWith(2, 'upload_font', { file_name: 'Corporate.woff', content: 'AQ==' })
  expect(refused).toEqual([
    'Corporate.woff: This is a WOFF or WOFF2 font, which PDFs cannot use. Upload the font as a TTF or OTF file.',
  ])
})

test('a failure inside Turtle names the file without the details', async () => {
  call.mockRejectedValue(new Error('network down'))

  const refused = await uploadFonts([new File([], 'Corporate.ttf')])

  expect(refused).toEqual(['Corporate.ttf could not be uploaded. Try again.'])
})
