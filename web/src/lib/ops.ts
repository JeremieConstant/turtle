import type { Operations } from './operations.gen'

export type OperationName = keyof Operations
export type Input<N extends OperationName> = Operations[N]['input']
export type Output<N extends OperationName> = Operations[N]['output']

/** The kinds of error an operation can end with. */
export type ErrorKind = 'not_found' | 'not_permitted' | 'invalid_input' | 'conflict' | 'internal'

/** An operation that did not happen, with a message meant for people. */
export class OperationError extends Error {
  constructor(
    readonly kind: ErrorKind,
    message: string,
  ) {
    super(message)
  }
}

/** Calls an operation of the catalogue (ADR-0005). The session cookie goes along by itself. */
export async function call<N extends OperationName>(name: N, input: Input<N>): Promise<Output<N>> {
  let response: Response
  try {
    response = await fetch(`/api/${name}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(input),
    })
  } catch {
    throw unreachable()
  }
  const body = await response.json().catch(() => null)
  return answer<N>(response.status, body)
}

/** A ZIP uploaded ahead of the import that reads it. */
export type Uploaded = { upload_id: string; bytes: number }

/**
 * Uploads a ZIP ahead of the import that reads it (ADR-0014), telling
 * `uploaded` what share of it has been sent, from 0 to 1, as it goes. The
 * file goes as it is, not in base64, so the browser never holds it whole;
 * the answer's `upload_id` is given to the import in place of the content.
 * fetch does not tell how much is sent, so it goes by XMLHttpRequest.
 */
export function upload(file: Blob, uploaded: (share: number) => void): Promise<Uploaded> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest()
    request.open('POST', '/api/uploads')
    request.setRequestHeader('content-type', 'application/zip')
    request.upload.addEventListener('progress', event => {
      if (event.lengthComputable && event.total > 0) uploaded(event.loaded / event.total)
    })
    request.addEventListener('load', () => {
      let body: unknown = null
      try {
        body = JSON.parse(request.responseText)
      } catch {
        // An answer that is not JSON is told by its status alone.
      }
      // Turtle tells why in JSON; what stands in front of it does not.
      if (request.status === 413 && body === null) {
        reject(
          new OperationError(
            'invalid_input',
            'What stands in front of this Turtle, such as a reverse proxy, takes no file this large. Ask whoever runs Turtle to raise its limit.',
          ),
        )
        return
      }
      try {
        resolve(answered<Uploaded>(request.status, body))
      } catch (caught) {
        reject(caught)
      }
    })
    request.addEventListener('error', () => reject(unreachable()))
    request.send(file)
  })
}

/** The output of an operation the server answered with `status`, or the error it ended with. */
function answer<N extends OperationName>(status: number, body: unknown): Output<N> {
  return answered<Output<N>>(status, body)
}

/** What the server answered with `status`, or the error it told instead. */
function answered<T>(status: number, body: unknown): T {
  if (status >= 200 && status < 300) return body as T
  const error = (body as { error?: { kind?: ErrorKind; message?: string } } | null)?.error
  throw new OperationError(error?.kind ?? 'internal', error?.message ?? `The server answered ${status}.`)
}

function unreachable(): OperationError {
  return new OperationError('internal', 'Turtle cannot be reached. Check your connection and try again.')
}
