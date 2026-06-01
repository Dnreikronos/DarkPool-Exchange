// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act, waitFor } from '@testing-library/react'

const DARKPOOL = '0x1111111111111111111111111111111111111111'
const WETH = '0x2222222222222222222222222222222222222222'
const USDC = '0x3333333333333333333333333333333333333333'
const TRADER = '0x4444444444444444444444444444444444444444'

vi.mock('@/lib/config', () => ({
  config: {
    useMocks: false,
    chainId: 31337,
    contracts: {
      darkPool: '0x1111111111111111111111111111111111111111',
      verifierProxy: '0x0',
      weth: '0x2222222222222222222222222222222222222222',
      usdc: '0x3333333333333333333333333333333333333333',
    },
  },
}))

vi.mock('@/lib/wallet/hooks', () => ({
  useWallet: () => ({
    address: '0x4444444444444444444444444444444444444444',
    status: 'connected',
    isConnected: true,
    isConnecting: false,
    connect: () => {},
    disconnect: () => {},
  }),
}))

// Controllable chain-state: allowances drive the approve decision; paused
// flows through to the controller interface.
const chainState = { allowances: { weth: '0', usdc: '0' }, paused: false, refetch: vi.fn() }
vi.mock('./useDepositChainState', () => ({
  useDepositChainState: () => chainState,
}))

const writeContractAsync = vi.fn()
vi.mock('wagmi', () => ({
  useWriteContract: () => ({ writeContractAsync }),
  useConfig: () => ({}),
}))

const waitForTransactionReceipt = vi.fn().mockResolvedValue({ status: 'success' })
vi.mock('wagmi/actions', () => ({
  waitForTransactionReceipt: (...a: unknown[]) => waitForTransactionReceipt(...a),
}))

import { useLiveDepositController, useLiveWithdrawController } from './live-controllers'

beforeEach(() => {
  writeContractAsync.mockReset()
  writeContractAsync.mockResolvedValue('0xhash')
  waitForTransactionReceipt.mockClear()
  chainState.allowances = { weth: '0', usdc: '0' }
  chainState.paused = false
  chainState.refetch.mockClear()
})

describe('useLiveDepositController', () => {
  it('approves the EXACT amount then deposits when allowance is insufficient', async () => {
    const { result } = renderHook(() => useLiveDepositController(true))
    await act(async () => {
      result.current.start({ token: 'USDC', amount: '100' })
    })
    await waitFor(() => expect(result.current.stage.kind).toBe('confirmed'))

    const approve = writeContractAsync.mock.calls.find((c) => c[0].functionName === 'approve')
    const deposit = writeContractAsync.mock.calls.find((c) => c[0].functionName === 'deposit')
    expect(approve?.[0]).toMatchObject({ address: USDC, args: [DARKPOOL, 100_000000n] })
    expect(deposit?.[0]).toMatchObject({ address: DARKPOOL, args: [USDC, 100_000000n] })
    // Never an infinite approval.
    expect(approve?.[0].args[1]).toBe(100_000000n)
    expect(chainState.refetch).toHaveBeenCalled()
  })

  it('skips approve when allowance already covers the amount', async () => {
    chainState.allowances = { weth: '0', usdc: '1000' }
    const { result } = renderHook(() => useLiveDepositController(true))
    await act(async () => {
      result.current.start({ token: 'USDC', amount: '100' })
    })
    await waitFor(() => expect(result.current.stage.kind).toBe('confirmed'))
    expect(writeContractAsync.mock.calls.some((c) => c[0].functionName === 'approve')).toBe(false)
    expect(writeContractAsync.mock.calls.some((c) => c[0].functionName === 'deposit')).toBe(true)
  })

  it('passes through signing then mining phases for the deposit step', async () => {
    chainState.allowances = { weth: '0', usdc: '1000' }
    let resolveReceipt: (v: unknown) => void = () => {}
    waitForTransactionReceipt.mockImplementationOnce(
      () => new Promise((res) => (resolveReceipt = res))
    )
    const { result } = renderHook(() => useLiveDepositController(true))
    await act(async () => {
      result.current.start({ token: 'USDC', amount: '100' })
    })
    // hash resolved, receipt pending → mining
    await waitFor(() =>
      expect(result.current.stage).toMatchObject({ kind: 'submitting', phase: 'mining' })
    )
    await act(async () => {
      resolveReceipt({ status: 'success' })
    })
    await waitFor(() => expect(result.current.stage.kind).toBe('confirmed'))
  })

  it('maps a wallet rejection to an error stage', async () => {
    writeContractAsync.mockRejectedValueOnce({ cause: { name: 'UserRejectedRequestError' } })
    const { result } = renderHook(() => useLiveDepositController(true))
    await act(async () => {
      result.current.start({ token: 'USDC', amount: '100' })
    })
    await waitFor(() => expect(result.current.stage.kind).toBe('error'))
    expect(result.current.stage.errorMessage).toMatch(/rejected/i)
  })

  it('surfaces paused from chain-state', () => {
    chainState.paused = true
    const { result } = renderHook(() => useLiveDepositController(true))
    expect(result.current.isPaused).toBe(true)
  })

  it('rejects a non-positive / unparseable amount as a zero-amount error', async () => {
    const { result } = renderHook(() => useLiveDepositController(true))
    await act(async () => {
      result.current.start({ token: 'USDC', amount: 'not-a-number' })
    })
    await waitFor(() => expect(result.current.stage.kind).toBe('error'))
    expect(writeContractAsync).not.toHaveBeenCalled()
  })
})

describe('useLiveWithdrawController', () => {
  it('calls withdraw with the exact raw amount and confirms', async () => {
    const { result } = renderHook(() => useLiveWithdrawController(true))
    await act(async () => {
      result.current.start({ token: 'WETH', amount: '1.5' })
    })
    await waitFor(() => expect(result.current.stage.kind).toBe('confirmed'))
    const withdraw = writeContractAsync.mock.calls.find((c) => c[0].functionName === 'withdraw')
    expect(withdraw?.[0]).toMatchObject({ address: DARKPOOL, args: [WETH, 1_500000000000000000n] })
    expect(writeContractAsync.mock.calls.some((c) => c[0].functionName === 'approve')).toBe(false)
  })

  it('maps a revert to an error stage', async () => {
    writeContractAsync.mockRejectedValueOnce({
      cause: { name: 'ContractFunctionRevertedError', reason: 'insufficient balance' },
    })
    const { result } = renderHook(() => useLiveWithdrawController(true))
    await act(async () => {
      result.current.start({ token: 'WETH', amount: '1.5' })
    })
    await waitFor(() => expect(result.current.stage.kind).toBe('error'))
    expect(result.current.stage.errorMessage).toMatch(/insufficient/i)
  })
})
