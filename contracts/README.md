# DarkPool Contracts

Solidity contracts for the ZK Dark Pool DEX, built with [Foundry](https://book.getfoundry.sh/).

## Contracts

| Contract | Purpose |
|---|---|
| `DarkPool.sol` | Escrow + batch settlement. Traders deposit ERC20s, operator submits batches with Groth16 proof. 5 bps protocol fee. |
| `VerifierProxy.sol` | Governance router — allows rotating the verifier without redeploying DarkPool. |
| `Groth16Verifier.sol` | BN254 proof verifier. VK is immutable at deploy. |
| `HyperNovaDeciderVerifier.sol` | Stub IVC verifier (not for mainnet). |

## Build & Test

```shell
forge build
forge test -vv
```

## Deploy

### Local (anvil)

```shell
just anvil          # start local chain
just deploy         # deploy to localhost:8545
```

### Remote (testnet / mainnet)

Set the following in your `.env` at the repo root:

```shell
RPC_URL=https://eth-sepolia.g.alchemy.com/v2/<YOUR_API_KEY>
PRIVATE_KEY=0x<deployer private key>
FEE_RECIPIENT=<address to receive protocol fees>
VK_JSON_PATH=./test/fixtures/vk.json
OPERATOR_PUBKEY_HEX=0x<33-byte compressed or 65-byte uncompressed SEC1 pubkey>
# Optional (defaults to deployer; required on mainnet):
# VERIFIER_GOVERNOR=<timelock or multisig address>
```

Then run:

```shell
just deploy-remote
```

## Deployment JSON

Each deploy writes `contracts/deployments/{chainId}.json` with the deployed addresses and metadata. The frontend imports this statically to resolve contract addresses per network.

### Format

```json
{
  "darkPool": "0x...",
  "verifierProxy": "0x...",
  "groth16Verifier": "0x...",
  "hypernovaDeciderVerifier": "0x...",
  "deployedAt": 1748189000,
  "blockNumber": 10920379,
  "gitSha": "40a8187"
}
```

| Field | Type | Description |
|---|---|---|
| `darkPool` | `address` | Main escrow + settlement contract |
| `verifierProxy` | `address` | Proxy routing verification calls |
| `groth16Verifier` | `address` | Groth16 proof verifier |
| `hypernovaDeciderVerifier` | `address` | IVC decider verifier (stub on testnet) |
| `deployedAt` | `uint256` | Unix timestamp of the deploy block |
| `blockNumber` | `uint256` | Block number of the deploy |
| `gitSha` | `string` | Short git SHA at deploy time |

### Committed deployments

- `deployments/31337.json` — localhost (anvil) deployment for local dev and E2E tests.
