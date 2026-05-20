// Single source of truth for the inline error copy. Codes come from
// validate.ts; the messages here follow the DESIGN-INSPIRATIONS tone:
// informative, no apology, no exclamation.

import { MIN_PRICE, MIN_SIZE, QUOTE_TOKEN } from './policy'
import type { ValidationCode } from './validate'

export function errorMessage(code: ValidationCode): string {
  switch (code) {
    case 'price-required':
      return 'Enter a price.'
    case 'price-invalid':
      return 'Price must be a positive number.'
    case 'price-below-min':
      return `Price must be at least ${MIN_PRICE} ${QUOTE_TOKEN}.`
    case 'size-required':
      return 'Enter a size.'
    case 'size-invalid':
      return 'Size must be a positive number.'
    case 'size-below-min':
      return `Size must be at least ${MIN_SIZE}.`
    case 'insufficient-balance':
      return 'Insufficient balance.'
    case 'wallet-disconnected':
      return 'Connect a wallet to place orders.'
  }
}
