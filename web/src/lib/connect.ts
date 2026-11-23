import type { Operations } from './operations.gen'

/** An app's request to connect, as the consent page answers it. */
export type ConnectionRequest = Omit<Operations['answer_connection_request']['input'], 'allow' | 'only_reading' | 'area_ids'>

/**
 * Reads the request an app sent to Turtle's authorization endpoint, which
 * passes it on in the consent page's address. Without a client ID it names
 * no client, and Turtle says the request is not valid, as it does for any
 * client it does not know.
 */
export function connectionRequest(query: string): ConnectionRequest {
  const parameters = new URLSearchParams(query)
  return {
    client_id: parameters.get('client_id') ?? '',
    redirect_uri: parameters.get('redirect_uri'),
    code_challenge: parameters.get('code_challenge'),
    code_challenge_method: parameters.get('code_challenge_method'),
    state: parameters.get('state'),
    scope: parameters.get('scope'),
  }
}
