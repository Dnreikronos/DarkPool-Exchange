import { describe, it, expect } from 'vitest'
import { decrypt, PrivateKey } from 'eciesjs'
import { encryptOrder } from './ecies'
import { hexToBytes } from './hex'
import testVector from './fixtures/ecies-test-vector.json'

describe('encryptOrder', () => {
  it('roundtrips: encrypt then decrypt yields original plaintext', () => {
    const sk = new PrivateKey()
    const pk = sk.publicKey.toBytes(false)
    const plaintext = new TextEncoder().encode('{"pair":"ETH-USD","side":0}')

    const ciphertext = encryptOrder(plaintext, pk)
    const decrypted = decrypt(sk.secret, ciphertext)

    expect(new TextDecoder().decode(decrypted)).toBe('{"pair":"ETH-USD","side":0}')
  })

  it('works with compressed pubkey (33 bytes)', () => {
    const sk = new PrivateKey()
    const pk = sk.publicKey.toBytes(true)
    expect(pk.length).toBe(33)

    const plaintext = new TextEncoder().encode('test')
    const ciphertext = encryptOrder(plaintext, pk)
    const decrypted = decrypt(sk.secret, ciphertext)

    expect(new TextDecoder().decode(decrypted)).toBe('test')
  })

  it('works with uncompressed pubkey (65 bytes)', () => {
    const sk = new PrivateKey()
    const pk = sk.publicKey.toBytes(false)
    expect(pk.length).toBe(65)

    const plaintext = new TextEncoder().encode('test')
    const ciphertext = encryptOrder(plaintext, pk)
    const decrypted = decrypt(sk.secret, ciphertext)

    expect(new TextDecoder().decode(decrypted)).toBe('test')
  })

  it('rejects pubkey with invalid length', () => {
    const badKey = new Uint8Array(32)
    const plaintext = new TextEncoder().encode('test')

    expect(() => encryptOrder(plaintext, badKey)).toThrow('expected 33')
  })

  it('rejects empty pubkey', () => {
    const plaintext = new TextEncoder().encode('test')
    expect(() => encryptOrder(plaintext, new Uint8Array(0))).toThrow('got 0')
  })

  it('ciphertext is larger than plaintext', () => {
    const sk = new PrivateKey()
    const pk = sk.publicKey.toBytes(false)
    const plaintext = new TextEncoder().encode('hello')

    const ciphertext = encryptOrder(plaintext, pk)
    expect(ciphertext.length).toBeGreaterThan(plaintext.length)
  })
})

describe('cross-language compatibility', () => {
  const skBytes = hexToBytes(testVector.secretKeyHex)
  const pkBytes = hexToBytes(testVector.publicKeyHex)
  const plaintextJson = JSON.stringify(testVector.plaintext)
  const ciphertextBytes = hexToBytes(testVector.ciphertextHex)

  it('TS decrypts Rust-generated ciphertext', () => {
    const decrypted = decrypt(skBytes, ciphertextBytes)
    const decryptedJson = new TextDecoder().decode(decrypted)
    expect(JSON.parse(decryptedJson)).toEqual(testVector.plaintext)
  })

  it('TS encrypt → TS decrypt roundtrip with Rust keypair', () => {
    const plaintext = new TextEncoder().encode(plaintextJson)
    const ciphertext = encryptOrder(plaintext, pkBytes)
    const decrypted = decrypt(skBytes, ciphertext)
    expect(new TextDecoder().decode(decrypted)).toBe(plaintextJson)
  })
})
