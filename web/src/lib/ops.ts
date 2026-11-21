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
    throw new OperationError('internal', 'Turtle cannot be reached. Check your connection and try again.')
  }
  const body = await response.json().catch(() => null)
  if (response.ok) return body as Output<N>
  const error = body?.error
  throw new OperationError(error?.kind ?? 'internal', error?.message ?? `The server answered ${response.status}.`)
}
