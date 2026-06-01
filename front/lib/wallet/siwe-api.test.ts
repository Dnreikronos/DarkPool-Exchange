import { describe, it, expect } from 'vitest'
import { fetchNonce, verifySiwe, SiweApiError } from './siwe-api'

interface FetchCall {
  url: string
  init: RequestInit
}

function captureFetch(make: (call: FetchCall) => Response) {
  const calls: FetchCall[] = []
  const fetchImpl = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const call = { url: String(input), init: init ?? {} }
    calls.push(call)
    return make(call)
  }) as unknown as typeof fetch
  return { fetchImpl, calls }
}

const API = 'http://localhost:8080'

describe('fetchNonce', () => {
  it('GETs /v1/auth/nonce and returns the nonce string', async () => {
    const { fetchImpl, calls } = captureFetch(
      () => new Response(JSON.stringify({ nonce: 'abc123NONCE' }), { status: 200 })
    )
    const nonce = await fetchNonce(API, fetchImpl)
    expect(nonce).toBe('abc123NONCE')
    expect(calls[0].url).toBe('http://localhost:8080/v1/auth/nonce')
    expect(calls[0].init.method ?? 'GET').toBe('GET')
  })

  it('normalises a trailing slash in the base URL', async () => {
    const { fetchImpl, calls } = captureFetch(
      () => new Response(JSON.stringify({ nonce: 'n' }), { status: 200 })
    )
    await fetchNonce('http://localhost:8080/', fetchImpl)
    expect(calls[0].url).toBe('http://localhost:8080/v1/auth/nonce')
  })

  it('throws SiweApiError with the status on a non-2xx response', async () => {
    const { fetchImpl } = captureFetch(
      () => new Response(JSON.stringify({ code: 8, message: 'nonce store full' }), { status: 429 })
    )
    await expect(fetchNonce(API, fetchImpl)).rejects.toMatchObject({
      name: 'SiweApiError',
      status: 429,
    })
  })

  it('throws when the response is missing the nonce field', async () => {
    const { fetchImpl } = captureFetch(() => new Response(JSON.stringify({}), { status: 200 }))
    await expect(fetchNonce(API, fetchImpl)).rejects.toBeInstanceOf(SiweApiError)
  })
})

describe('verifySiwe', () => {
  it('POSTs {message, signature} and maps expires_at -> expiresAt', async () => {
    const { fetchImpl, calls } = captureFetch(
      () =>
        new Response(
          JSON.stringify({ token: 'jwt', expires_at: 1893456000, address: '0xabc' }),
          { status: 200 }
        )
    )
    const result = await verifySiwe(API, { message: 'msg', signature: '0xsig' }, fetchImpl)
    expect(result).toEqual({ token: 'jwt', expiresAt: 1893456000, address: '0xabc' })
    expect(calls[0].url).toBe('http://localhost:8080/v1/auth/verify')
    expect(calls[0].init.method).toBe('POST')
    expect((calls[0].init.headers as Record<string, string>)['content-type']).toBe(
      'application/json'
    )
    expect(JSON.parse(calls[0].init.body as string)).toEqual({
      message: 'msg',
      signature: '0xsig',
    })
  })

  it('throws SiweApiError with the status on a 401', async () => {
    const { fetchImpl } = captureFetch(
      () => new Response(JSON.stringify({ message: 'nonce invalid' }), { status: 401 })
    )
    await expect(
      verifySiwe(API, { message: 'm', signature: '0xs' }, fetchImpl)
    ).rejects.toMatchObject({ name: 'SiweApiError', status: 401 })
  })
})
