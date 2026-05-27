import { ECIES_CONFIG, encrypt } from 'eciesjs'

// Match Rust ecies 0.2 "pure" defaults: AES-256-GCM, uncompressed ephemeral keys
ECIES_CONFIG.symmetricAlgorithm = 'aes-256-gcm'
ECIES_CONFIG.isEphemeralKeyCompressed = false
ECIES_CONFIG.isHkdfKeyCompressed = false

const SEC1_COMPRESSED = 33
const SEC1_UNCOMPRESSED = 65

export function encryptOrder(orderJson: Uint8Array, operatorPubkey: Uint8Array): Uint8Array {
  const len = operatorPubkey.length
  if (len !== SEC1_COMPRESSED && len !== SEC1_UNCOMPRESSED) {
    throw new Error(`expected 33 (compressed) or 65 (uncompressed) SEC1 bytes, got ${len}`)
  }

  return encrypt(operatorPubkey, orderJson)
}
