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

/**
 * Calls an operation as `call` does, telling `uploaded` what share of the
 * input has been sent, from 0 to 1, as it goes: for an input that takes a
 * while to send, such as a ZIP to import. fetch does not tell this, so the
 * input goes by XMLHttpRequest.
 */
export function callTellingUpload<N extends OperationName>(
  name: N,
  input: Input<N>,
  uploaded: (share: number) => void,
): Promise<Output<N>> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest()
    request.open('POST', `/api/${name}`)
    request.setRequestHeader('content-type', 'application/json')
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
      try {
        resolve(answer<N>(request.status, body))
      } catch (caught) {
        reject(caught)
      }
    })
    request.addEventListener('error', () => reject(unreachable()))
    request.send(JSON.stringify(input))
  })
}

/** The output of an operation the server answered with `status`, or the error it ended with. */
function answer<N extends OperationName>(status: number, body: unknown): Output<N> {
  if (status >= 200 && status < 300) return body as Output<N>
  const error = (body as { error?: { kind?: ErrorKind; message?: string } } | null)?.error
  throw new OperationError(error?.kind ?? 'internal', error?.message ?? `The server answered ${status}.`)
}

function unreachable(): OperationError {
  return new OperationError('internal', 'Turtle cannot be reached. Check your connection and try again.')
}
