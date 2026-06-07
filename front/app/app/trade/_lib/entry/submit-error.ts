// Maps a DarkPoolError (mirrors tonic::Code; see crates/dp-api/src/rest.rs
// ApiError::into_response) to user-facing copy for the order-entry inline
// error area, plus a structured detail the UI uses for the 429 retry hint
// and the 5xx collapsible technical block (x-request-id). Tone follows
// DESIGN-INSPIRATIONS: informative, no apology, no exclamation.

import { DarkPoolError, type DarkPoolErrorName } from '@/lib/api-client'

export interface SubmitErrorDetail {
  code: number
  codeName: DarkPoolErrorName
  httpStatus: number | null
  requestId: string | null
  /** `retry-after` header value: seconds or an HTTP-date string. */
  retryAfter: string | null
  serverMessage: string
}

const MESSAGES: Record<DarkPoolErrorName, string> = {
  OK: 'Unexpected response from the server. Try again.',
  CANCELLED: 'The request was cancelled. Try submitting again.',
  UNKNOWN: 'Server error. Try again.',
  INVALID_ARGUMENT: 'Order rejected: the engine refused these values.',
  DEADLINE_EXCEEDED: 'The request timed out. Try again.',
  NOT_FOUND: 'Market not found.',
  ALREADY_EXISTS: 'This order was already submitted.',
  PERMISSION_DENIED: 'Not allowed: this key cannot place orders.',
  RESOURCE_EXHAUSTED: 'Too many requests. Slow down and retry.',
  FAILED_PRECONDITION: 'Order rejected: the market is not accepting it right now.',
  ABORTED: 'The order conflicted with a concurrent change. Try again.',
  OUT_OF_RANGE: 'Order rejected: a value is out of the accepted range.',
  UNIMPLEMENTED: 'This action is not available yet.',
  INTERNAL: 'Something went wrong on the server. Try again.',
  UNAVAILABLE: 'The engine is unreachable. Retry in a moment.',
  DATA_LOSS: 'Something went wrong on the server. Try again.',
  UNAUTHENTICATED: 'Authentication failed: check the API key.',
}

export function submitErrorMessage(err: DarkPoolError): string {
  return MESSAGES[err.codeName] ?? MESSAGES.UNKNOWN
}

export function toSubmitErrorDetail(err: DarkPoolError): SubmitErrorDetail {
  return {
    code: err.code,
    codeName: err.codeName,
    httpStatus: err.httpStatus,
    requestId: err.requestId,
    retryAfter: err.retryAfter,
    serverMessage: err.message,
  }
}

export function mapSubmissionError(err: unknown): { message: string; detail?: SubmitErrorDetail } {
  if (err instanceof DarkPoolError) {
    return { message: submitErrorMessage(err), detail: toSubmitErrorDetail(err) }
  }
  if (err instanceof Error) return { message: err.message }
  return { message: String(err) }
}
