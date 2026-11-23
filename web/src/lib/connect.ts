import type { Operations } from './operations.gen'

/** An app's request to connect, as the consent page answers it. */
export type ConnectionRequest = Omit<Operations['answer_connection_request']['input'], 'allow'>

/**
 * Reads the request an app sent to Turtle's authorization endpoint, which
 * passes it on in the consent page's address. Without a client ID there is
 * no request.
 */
export function connectionRequest(query: string): ConnectionRequest | null {
  const parameters = new URLSearchParams(query)
  const client_id = parameters.get('client_id')
  if (!client_id) return null
  return {
    client_id,
    redirect_uri: parameters.get('redirect_uri'),
    code_challenge: parameters.get('code_challenge'),
    code_challenge_method: parameters.get('code_challenge_method'),
    state: parameters.get('state'),
  }
}
