import { describe, expect, it } from 'vitest'

import { mapTxError } from './errors'

// Synthetic errors mirroring the viem/wagmi shapes the write path throws.
// viem wraps the root cause in an outer error whose `.cause` chain ends in
// a `ContractFunctionRevertedError` (carrying `.reason` and/or
// `.data.errorName`) or a `UserRejectedRequestError`. We don't import the
// real classes — `mapTxError` matches on the structural fields, so tests
// stay chain-free.

describe('mapTxError', () => {
  it('maps a wallet rejection (nested UserRejectedRequestError)', () => {
    const err = {
      name: 'TransactionExecutionError',
      shortMessage: 'User rejected the request.',
      cause: { name: 'UserRejectedRequestError', code: 4001 },
    }
    expect(mapTxError(err).reason).toBe('user-rejected')
  })

  it('maps a wallet rejection by EIP-1193 code 4001 alone', () => {
    expect(mapTxError({ code: 4001, message: 'denied' }).reason).toBe('user-rejected')
  })

  it('maps the contract "zero amount" revert', () => {
    const err = {
      name: 'ContractFunctionExecutionError',
      cause: {
        name: 'ContractFunctionRevertedError',
        reason: 'zero amount',
        shortMessage: 'execution reverted: zero amount',
      },
    }
    expect(mapTxError(err).reason).toBe('zero-amount')
  })

  it('maps an ERC20 insufficient-allowance revert to an allowance race (legacy string)', () => {
    const err = {
      name: 'ContractFunctionExecutionError',
      cause: { name: 'ContractFunctionRevertedError', reason: 'ERC20: insufficient allowance' },
    }
    expect(mapTxError(err).reason).toBe('allowance-race')
  })

  it('maps an ERC20InsufficientAllowance custom error to an allowance race', () => {
    const err = {
      name: 'ContractFunctionExecutionError',
      cause: {
        name: 'ContractFunctionRevertedError',
        data: { errorName: 'ERC20InsufficientAllowance' },
      },
    }
    expect(mapTxError(err).reason).toBe('allowance-race')
  })

  it('maps the contract "insufficient balance" revert', () => {
    const err = {
      name: 'ContractFunctionExecutionError',
      cause: { name: 'ContractFunctionRevertedError', reason: 'insufficient balance' },
    }
    expect(mapTxError(err).reason).toBe('insufficient')
  })

  it('maps an EnforcedPause custom error to paused', () => {
    const err = {
      name: 'ContractFunctionExecutionError',
      cause: { name: 'ContractFunctionRevertedError', data: { errorName: 'EnforcedPause' } },
    }
    expect(mapTxError(err).reason).toBe('paused')
  })

  it('maps a gas-estimation execution revert with no decoded reason to reverted', () => {
    const err = {
      name: 'EstimateGasExecutionError',
      shortMessage: 'Execution reverted for an unknown reason.',
    }
    expect(mapTxError(err).reason).toBe('reverted')
  })

  it('falls back to unknown for an unrecognised error', () => {
    expect(mapTxError(new Error('boom')).reason).toBe('unknown')
    expect(mapTxError(null).reason).toBe('unknown')
    expect(mapTxError(undefined).reason).toBe('unknown')
  })

  it('returns a non-empty human message for every reason', () => {
    const samples: unknown[] = [
      { cause: { name: 'UserRejectedRequestError' } },
      { cause: { reason: 'zero amount' } },
      { cause: { reason: 'ERC20: insufficient allowance' } },
      { cause: { reason: 'insufficient balance' } },
      { cause: { data: { errorName: 'EnforcedPause' } } },
      { name: 'EstimateGasExecutionError', shortMessage: 'reverted' },
      new Error('boom'),
    ]
    for (const s of samples) {
      const { message } = mapTxError(s)
      expect(typeof message).toBe('string')
      expect(message.length).toBeGreaterThan(0)
    }
  })
})
