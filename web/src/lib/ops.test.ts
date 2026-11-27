import { afterEach, expect, test, vi } from 'vitest'
import { OperationError, upload } from './ops'

/** A stand-in for XMLHttpRequest that answers with `status` and `body` once sent, having told `progress` of the upload. */
function fakeRequest(status: number, body: string, progress: [number, number][]) {
  const sent: { url?: string; body?: Blob; headers: Record<string, string> } = { headers: {} }
  class FakeRequest extends EventTarget {
    status = 0
    responseText = ''
    upload = new EventTarget()
    open(_method: string, url: string) {
      sent.url = url
    }
    setRequestHeader(name: string, value: string) {
      sent.headers[name] = value
    }
    send(input: Blob) {
      sent.body = input
      for (const [loaded, total] of progress) {
        this.upload.dispatchEvent(Object.assign(new Event('progress'), { lengthComputable: true, loaded, total }))
      }
      this.status = status
      this.responseText = body
      this.dispatchEvent(new Event('load'))
    }
  }
  vi.stubGlobal('XMLHttpRequest', FakeRequest)
  return sent
}

afterEach(() => vi.unstubAllGlobals())

const zip = new Blob([new Uint8Array([0x50, 0x4b, 0x03, 0x04])])

test('a ZIP is uploaded as it is, telling what share is sent, and answers with the ID to import it by', async () => {
  const sent = fakeRequest(200, '{"upload_id":"u1","bytes":4}', [
    [25, 100],
    [100, 100],
  ])
  const shares: number[] = []

  const uploaded = await upload(zip, share => shares.push(share))

  expect(uploaded).toEqual({ upload_id: 'u1', bytes: 4 })
  expect(shares).toEqual([0.25, 1])
  expect(sent).toEqual({ url: '/api/uploads', body: zip, headers: { 'content-type': 'application/zip' } })
})

test('an upload refused by what stands in front of Turtle says so', async () => {
  fakeRequest(413, '<html>Request Entity Too Large</html>', [])

  await expect(upload(zip, () => {})).rejects.toEqual(
    new OperationError(
      'invalid_input',
      'What stands in front of this Turtle, such as a reverse proxy, takes no file this large. Ask whoever runs Turtle to raise its limit.',
    ),
  )
})

test('an upload fails with the error the server tells', async () => {
  fakeRequest(400, '{"error":{"kind":"invalid_input","message":"This ZIP is larger than 2 GB."}}', [])

  await expect(upload(zip, () => {})).rejects.toEqual(new OperationError('invalid_input', 'This ZIP is larger than 2 GB.'))
})
