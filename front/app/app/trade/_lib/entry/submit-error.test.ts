import { describe, expect, it } from 'vitest'

import { DARK_POOL_ERROR_CODES, DarkPoolError } from '@/lib/api-client'

import { mapSubmissionError, submitErrorMessage, toSubmitErrorDetail } from './submit-error'

describe('submitErrorMessage', () => {
  const cases: Array<[keyof typeof DARK_POOL_ERROR_CODES, RegExp]> = [
    ['INVALID_ARGUMENT', /rejected/i],
    ['FAILED_PRECONDITION', /rejected/i],
    ['OUT_OF_RANGE', /rejected/i],
    ['UNAUTHENTICATED', /sign in|authenticate|api key/i],
    ['PERMISSION_DENIED', /not allowed|permission/i],
    ['NOT_FOUND', /not found/i],
    ['ALREADY_EXISTS', /already/i],
    ['RESOURCE_EXHAUSTED', /too many|rate/i],
    ['UNAVAILABLE', /unavailable|unreachable|retry/i],
    ['DEADLINE_EXCEEDED', /timed out|timeout/i],
    ['ABORTED', /try again|conflict/i],
    ['UNIMPLEMENTED', /not (yet )?available|unsupported/i],
    ['INTERNAL', /server error|something went wrong/i],
    ['UNKNOWN', /server error|something went wrong/i],
    ['DATA_LOSS', /server error|something went wrong/i],
    ['CANCELLED', /cancell?ed/i],
    ['OK', /unexpected|try again/i],
  ]
  it.each(cases)('maps %s to a specific message', (name, re) => {
    const err = new DarkPoolError(DARK_POOL_ERROR_CODES[name], 'raw server message')
    expect(submitErrorMessage(err)).toMatch(re)
  })
})

describe('toSubmitErrorDetail', () => {
  it('carries code/codeName/httpStatus/requestId/retryAfter', () => {
    const err = new DarkPoolError(DARK_POOL_ERROR_CODES.RESOURCE_EXHAUSTED, 'slow down', {
      httpStatus: 429,
      requestId: 'req-1',
      retryAfter: '30',
    })
    expect(toSubmitErrorDetail(err)).toEqual({
      code: DARK_POOL_ERROR_CODES.RESOURCE_EXHAUSTED,
      codeName: 'RESOURCE_EXHAUSTED',
      httpStatus: 429,
      requestId: 'req-1',
      retryAfter: '30',
      serverMessage: 'slow down',
    })
  })
})

describe('mapSubmissionError', () => {
  it('returns message + detail for a DarkPoolError', () => {
    const err = new DarkPoolError(DARK_POOL_ERROR_CODES.INTERNAL, 'boom', {
      httpStatus: 500,
      requestId: 'req-x',
    })
    const out = mapSubmissionError(err)
    expect(out.message).toMatch(/server error|something went wrong/i)
    expect(out.detail?.requestId).toBe('req-x')
  })

  it('passes through a plain Error message with no detail', () => {
    expect(mapSubmissionError(new Error('worker died'))).toEqual({ message: 'worker died' })
  })

  it('stringifies non-Error throwables', () => {
    expect(mapSubmissionError('nope')).toEqual({ message: 'nope' })
  })
})
