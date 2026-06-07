import { describe, expect, it } from 'vitest'

import { ONBOARDING_STEPS } from './copy'

describe('onboarding copy', () => {
  it('warns that clearing browser storage erases the local trade history (#101)', () => {
    const allBody = ONBOARDING_STEPS.flatMap((step) => step.body).join(' ')
    expect(allBody).toMatch(/history.*(this browser|locally)/i)
    expect(allBody).toMatch(/clearing (site|browser) (data|storage)/i)
  })
})
