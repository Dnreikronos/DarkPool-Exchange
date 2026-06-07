// Maps the viem/wagmi error thrown by an approve / deposit / withdraw write
// into one of the AC#4 categories, with brutalist copy (no colour, the
// failure reads in words). We match on the structural fields viem populates
// rather than importing its error classes, so the mapper is a pure function
// testable against synthetic error objects.
//
// viem nests the decoded revert in a `.cause` chain that bottoms out in a
// `ContractFunctionRevertedError` carrying `.reason` (legacy `require`
// strings) and/or `.data.errorName` (custom errors). A wallet rejection
// surfaces as a `UserRejectedRequestError` (EIP-1193 code 4001).

export type TxErrorReason =
  | 'user-rejected'
  | 'paused'
  | 'zero-amount'
  | 'allowance-race'
  | 'insufficient'
  | 'reverted'
  | 'unknown'

export interface TxError {
  reason: TxErrorReason
  message: string
}

const MESSAGES: Record<TxErrorReason, string> = {
  'user-rejected': 'Signature rejected in wallet.',
  paused: 'Contract is paused — deposits and withdrawals are suspended.',
  'zero-amount': 'Amount must be greater than zero.',
  'allowance-race': 'Allowance changed — re-approve and retry.',
  insufficient: 'Insufficient balance for this transaction.',
  reverted: 'Transaction reverted on-chain.',
  unknown: 'Transaction failed.',
}

/** Collect the searchable text + error names across the whole cause chain. */
function collect(err: unknown): { haystack: string; codes: Set<number> } {
  const parts: string[] = []
  const codes = new Set<number>()
  const seen = new Set<unknown>()

  let node: unknown = err
  while (node && typeof node === 'object' && !seen.has(node)) {
    seen.add(node)
    const o = node as Record<string, unknown>
    for (const key of ['name', 'shortMessage', 'message', 'reason', 'details', 'metaMessages']) {
      const v = o[key]
      if (typeof v === 'string') parts.push(v)
      else if (Array.isArray(v)) parts.push(v.filter((x) => typeof x === 'string').join(' '))
    }
    const data = o.data
    if (data && typeof data === 'object') {
      const errorName = (data as Record<string, unknown>).errorName
      if (typeof errorName === 'string') parts.push(errorName)
    }
    if (typeof o.code === 'number') codes.add(o.code)
    node = o.cause
  }

  return { haystack: parts.join('  ').toLowerCase(), codes }
}

export function mapTxError(err: unknown): TxError {
  const { haystack, codes } = collect(err)
  const has = (s: string) => haystack.includes(s)

  const reason: TxErrorReason =
    // Wallet rejection first: it is never a revert, and code 4001 is unambiguous.
    has('userrejectedrequest') || codes.has(4001) || has('user rejected') || has('user denied')
      ? 'user-rejected'
      : // Specific reverts before the generic catch-all.
        has('zero amount')
        ? 'zero-amount'
        : has('insufficientallowance') || has('insufficient allowance')
          ? 'allowance-race'
          : has('enforcedpause') || has('paused')
            ? 'paused'
            : has('insufficientbalance') ||
                has('insufficient balance') ||
                has('exceeds balance') ||
                has('transfer amount exceeds')
              ? 'insufficient'
              : has('revert') || has('executionerror') || has('contractfunction')
                ? 'reverted'
                : 'unknown'

  return { reason, message: MESSAGES[reason] }
}
