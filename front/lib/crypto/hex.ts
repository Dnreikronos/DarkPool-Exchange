export function hexToBytes(hex: string): Uint8Array {
  const trimmed = hex.replace(/^0x/i, '')
  if (trimmed.length % 2 !== 0) {
    throw new Error(`hex string must have even length, got ${trimmed.length}`)
  }
  if (!/^[0-9a-fA-F]*$/.test(trimmed)) {
    throw new Error('hex string contains non-hex characters')
  }
  const bytes = new Uint8Array(trimmed.length / 2)
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(trimmed.substring(i * 2, i * 2 + 2), 16)
  }
  return bytes
}
