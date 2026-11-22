import { toast } from 'svelte-sonner'
import { OperationError } from './ops'

/**
 * Says in a toast that an operation failed. The operation's own message is
 * written for people; anything else that went wrong gets `fallback`.
 */
export function reportFailure(caught: unknown, fallback: string) {
  toast.error(caught instanceof OperationError ? caught.message : fallback)
}
