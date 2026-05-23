/**
 * Copy for the three onboarding steps. Tone follows DESIGN-INSPIRATIONS:
 *  - Sentence case, no marketing voice, no exclamation marks.
 *  - Borrows the batch-auction explainer from CoW Swap (sec. "Direct
 *    genre peers"). Same protocol property, same vocabulary.
 *  - Steps lead with the property, then the consequence the trader feels.
 *
 * Step IDs are 1-indexed so the rendered `step-node` reads as `01`, `02`,
 * `03` per the design token.
 */

export interface OnboardingStep {
  /** Two-digit zero-padded id rendered inside the step-node. */
  id: '01' | '02' | '03'
  /** Bracketed-tag kicker above the title, e.g. `[ STEP 01 · PROTOCOL ]`. */
  kicker: string
  /** Display headline (Bebas Neue uppercase). */
  title: string
  /** Body paragraphs (sentence case, IBM Plex Mono). */
  body: readonly string[]
  /** Optional metadata line in `body-sm` (e.g. cadence or duration). */
  meta?: string
}

export const ONBOARDING_STEPS: readonly OnboardingStep[] = [
  {
    id: '01',
    kicker: '[ STEP 01 · PROTOCOL ]',
    title: 'BATCH AUCTIONS',
    body: [
      'Orders sit encrypted in a sealed batch. Every five seconds the engine clears the batch at a single price.',
      'Everyone in the same batch trades at the same price, so there is no race to be first and no value to extract by sequencing the queue.',
    ],
    meta: 'Batch cadence: 5 s.',
  },
  {
    id: '02',
    kicker: '[ STEP 02 · CUSTODY ]',
    title: 'OPERATOR CUSTODY',
    body: [
      'Deposit ERC-20 tokens to the DarkPool contract before trading. Balances are tracked inside the contract; the operator never holds funds on a centralized server.',
      'Withdraw at any time. Settlement of every batch is proven on-chain with a zero-knowledge proof — the operator cannot move funds outside the matched trades.',
    ],
    meta: 'On-chain settlement, off-chain matching.',
  },
  {
    id: '03',
    kicker: '[ STEP 03 · PROVING ]',
    title: 'PROOF IN THE BROWSER',
    body: [
      'Submitting an order builds a zero-knowledge proof of validity in this tab before it leaves your machine. That keeps your price and size hidden from the operator and from any external observer until the batch settles.',
      'Proof generation runs locally and takes roughly fifteen to thirty seconds. The submit button reports each stage so you can watch it progress.',
    ],
    meta: 'Local proving time: ~15–30 s.',
  },
]

export const ONBOARDING_BACK_LABEL = '[ BACK ]'
export const ONBOARDING_NEXT_LABEL = '[ NEXT ]'
export const ONBOARDING_DONE_LABEL = '[ START TRADING ]'
export const ONBOARDING_DIALOG_TITLE = 'Welcome to DarkPool.'
export const ONBOARDING_DIALOG_DESCRIPTION =
  'Three things to know before you place an order. Skip with the close icon at any time.'
