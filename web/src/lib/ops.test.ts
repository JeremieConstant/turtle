import { afterEach, expect, test, vi } from 'vitest'
import { callTellingUpload, OperationError } from './ops'

/** A stand-in for XMLHttpRequest that answers with `status` and `body` once sent, having told `progress` of the upload. */
function fakeRequest(status: number, body: string, progress: [number, number][]) {
  const sent: { url?: string; body?: string; headers: Record<string, string> } = { headers: {} }
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
    send(input: string) {
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

test('an operation called telling its upload says what share is sent and answers with its output', async () => {
  const sent = fakeRequest(200, '{"import_id":"i1","stage":"reading","done":0,"total":0}', [
    [25, 100],
    [100, 100],
  ])
  const shares: number[] = []

  const output = await callTellingUpload('start_import', { area_id: 'a1', content: 'UEsDBA==' }, share => shares.push(share))

  expect(output).toEqual({ import_id: 'i1', stage: 'reading', done: 0, total: 0 })
  expect(shares).toEqual([0.25, 1])
  expect(sent).toEqual({ url: '/api/start_import', body: '{"area_id":"a1","content":"UEsDBA=="}', headers: { 'content-type': 'application/json' } })
})

test('an operation called telling its upload fails with the error the server tells', async () => {
  fakeRequest(413, '{"error":{"kind":"invalid_input","message":"This ZIP is larger than 200 MB."}}', [])

  const failed = callTellingUpload('start_import', { area_id: 'a1', content: '' }, () => {})

  await expect(failed).rejects.toEqual(new OperationError('invalid_input', 'This ZIP is larger than 200 MB.'))
})
