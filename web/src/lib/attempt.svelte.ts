import { OperationError } from './ops'

/**
 * An operation a person starts from a form or button: busy while it runs, so
 * the button can be disabled, and what went wrong as a message for them.
 */
export class Attempt {
  busy = $state(false)
  error = $state<string | null>(null)

  /** Runs `operation`. A thrown OperationError becomes the error shown to the person. */
  async run(operation: () => Promise<void>) {
    this.busy = true
    this.error = null
    try {
      await operation()
    } catch (caught) {
      this.error = caught instanceof OperationError ? caught.message : 'Something went wrong. Try again.'
    } finally {
      this.busy = false
    }
  }
}
