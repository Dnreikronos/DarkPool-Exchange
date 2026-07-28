use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use alloy_network::{Ethereum, EthereumWallet, NetworkWallet, ReceiptResponse};
use alloy_primitives::{Address, Bytes};
use alloy_provider::{PendingTransactionBuilder, Provider};
use alloy_rpc_types::BlockNumberOrTag;
use tracing::Instrument;

use crate::abi::{DarkPool, MAX_MATCHES_PER_BATCH};
use crate::helpers::{settlement_match_to_sol, uuid_to_bytes32};
use crate::signer::TxSigner;
use crate::submitter::Submitter;
use crate::{SettlementError, SubmitBatchParams};

const GROTH16_PROOF_LEN: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettlementTxTransport {
    PrivateRpc,
    PublicMempool,
}

impl SettlementTxTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrivateRpc => "private_rpc",
            Self::PublicMempool => "public_mempool",
        }
    }
}

pub struct EthSubmitterConfig {
    /// Operator transaction-signing backend. Constructed via
    /// `dp_settlement::signer::from_uri` at boot so the raw private key
    /// stays inside the adapter (KMS-backed signers never expose it at
    /// all). Wrapped in `Arc` because the submitter holds it for the
    /// process lifetime and `EthereumWallet` derived from it is cloned
    /// per-call.
    pub signer: Arc<dyn TxSigner>,
    pub contract_address: String,
    pub chain_id: u64,
    pub gas_limit: Option<u64>,
    pub tx_transport: SettlementTxTransport,
}

pub struct EthSubmitter<ReadP, SubmitP = ReadP> {
    read_provider: ReadP,
    submit_provider: SubmitP,
    wallet: EthereumWallet,
    contract: Address,
    chain_id: u64,
    gas_limit: u64,
    tx_transport: SettlementTxTransport,
}

impl<P: Provider + Clone + Send + Sync> EthSubmitter<P> {
    pub fn new(provider: P, config: &EthSubmitterConfig) -> Result<Self, SettlementError> {
        Self::with_submit_provider(provider.clone(), provider, config)
    }
}

impl<ReadP: Provider + Send + Sync, SubmitP: Provider + Send + Sync> EthSubmitter<ReadP, SubmitP> {
    pub fn with_submit_provider(
        read_provider: ReadP,
        submit_provider: SubmitP,
        config: &EthSubmitterConfig,
    ) -> Result<Self, SettlementError> {
        let wallet = config.signer.wallet();
        let contract = Address::from_str(&config.contract_address)
            .map_err(|e| SettlementError::Rpc(e.to_string()))?;
        Ok(Self {
            read_provider,
            submit_provider,
            wallet,
            contract,
            chain_id: config.chain_id,
            gas_limit: config.gas_limit.unwrap_or(500_000),
            tx_transport: config.tx_transport,
        })
    }

    #[cfg(test)]
    pub fn pack_submit(params: &SubmitBatchParams) -> Result<Vec<u8>, SettlementError> {
        use crate::abi::submitBatchCall;
        use alloy_sol_types::SolCall;
        validate_groth16_proof_len(&params.proof)?;
        let sol_matches = build_sol_matches(params)?;
        let call = submitBatchCall {
            batchId: uuid_to_bytes32(params.batch_id),
            auctionId: uuid_to_bytes32(params.auction_id),
            proof: Bytes::copy_from_slice(&params.proof),
            publicInputs: params.public_inputs,
            matches: sol_matches,
        };
        Ok(call.abi_encode())
    }
}

fn validate_groth16_proof_len(proof: &[u8]) -> Result<(), SettlementError> {
    if proof.len() != GROTH16_PROOF_LEN {
        return Err(SettlementError::InvalidProofLength {
            expected: GROTH16_PROOF_LEN,
            actual: proof.len(),
        });
    }
    Ok(())
}

fn build_sol_matches(
    params: &SubmitBatchParams,
) -> Result<Vec<crate::abi::SolMatch>, SettlementError> {
    if params.matches.len() > MAX_MATCHES_PER_BATCH {
        return Err(SettlementError::TooManyMatches {
            count: params.matches.len(),
        });
    }
    params.matches.iter().map(settlement_match_to_sol).collect()
}

/// Build the tracing span for a single `submit` call. Extracted so
/// unit tests can verify the span's fields without standing up a real
/// alloy `Provider`. Field names follow the OTel HTTP / RPC
/// conventions where it makes sense.
fn build_submit_span(
    params: &SubmitBatchParams,
    tx_transport: SettlementTxTransport,
) -> tracing::Span {
    tracing::info_span!(
        "dp_settlement.eth_submit",
        batch_id = %params.batch_id,
        auction_id = %params.auction_id,
        match_count = params.matches.len(),
        tx_transport = tx_transport.as_str(),
    )
}

#[cfg(feature = "hypernova")]
fn build_session_span(
    params: &crate::SubmitSessionParams,
    tx_transport: SettlementTxTransport,
) -> tracing::Span {
    tracing::info_span!(
        "dp_settlement.eth_submit_session",
        session_id = %params.session_id,
        auction_id = %params.auction_id,
        n_steps = params.n_steps,
        match_count = params.matches.len(),
        tx_transport = tx_transport.as_str(),
    )
}

impl<ReadP, SubmitP> Submitter for EthSubmitter<ReadP, SubmitP>
where
    ReadP: Provider + Send + Sync + 'static,
    SubmitP: Provider + Send + Sync + 'static,
{
    fn submit<'a>(
        &'a self,
        params: &'a SubmitBatchParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        let span = build_submit_span(params, self.tx_transport);
        Box::pin(
            async move {
                validate_groth16_proof_len(&params.proof)?;
                let sol_matches = build_sol_matches(params)?;
                let sender = NetworkWallet::<Ethereum>::default_signer_address(&self.wallet);

                let nonce = self
                    .read_provider
                    .get_transaction_count(sender)
                    .pending()
                    .await
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;

                let latest_block = self
                    .read_provider
                    .get_block_by_number(BlockNumberOrTag::Latest)
                    .await
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?
                    .ok_or_else(|| SettlementError::Rpc("no latest block".into()))?;

                let base_fee = latest_block.header.base_fee_per_gas.ok_or_else(|| {
                    SettlementError::Rpc("chain does not support EIP-1559".into())
                })?;

                let tip = self
                    .read_provider
                    .get_max_priority_fee_per_gas()
                    .await
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;
                let fee_cap = u128::from(base_fee) * 2 + tip;

                let contract = DarkPool::new(self.contract, &self.submit_provider);
                let call = contract
                    .submitBatch(
                        uuid_to_bytes32(params.batch_id),
                        uuid_to_bytes32(params.auction_id),
                        Bytes::copy_from_slice(&params.proof),
                        params.public_inputs,
                        sol_matches,
                    )
                    .from(sender)
                    .nonce(nonce)
                    .gas(self.gas_limit)
                    .max_fee_per_gas(fee_cap)
                    .max_priority_fee_per_gas(tip)
                    .chain_id(self.chain_id);

                let pending = self
                    .submit_provider
                    .send_transaction(call.into_transaction_request())
                    .await
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;
                let tx_hash = *pending.tx_hash();

                let receipt =
                    PendingTransactionBuilder::new(self.read_provider.root().clone(), tx_hash)
                        .get_receipt()
                        .await
                        .map_err(|e| SettlementError::Rpc(e.to_string()))?;
                receipt
                    .ensure_success()
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;

                Ok(format!("{:#x}", receipt.transaction_hash))
            }
            .instrument(span),
        )
    }

    #[cfg(feature = "hypernova")]
    fn submit_session<'a>(
        &'a self,
        params: &'a crate::SubmitSessionParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        let span = build_session_span(params, self.tx_transport);
        Box::pin(
            async move {
                let sol_matches = params
                    .matches
                    .iter()
                    .map(settlement_match_to_sol)
                    .collect::<Result<Vec<_>, _>>()?;
                if sol_matches.len() > MAX_MATCHES_PER_BATCH {
                    return Err(SettlementError::TooManyMatches {
                        count: sol_matches.len(),
                    });
                }

                let sender = NetworkWallet::<Ethereum>::default_signer_address(&self.wallet);
                let contract = DarkPool::new(self.contract, &self.submit_provider);

                // submitSession: commits the IVC proof and its final state; the
                // matches are bound by settleAuction recomputing zN[3] from them.
                let nonce_a = self
                    .read_provider
                    .get_transaction_count(sender)
                    .pending()
                    .await
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;
                let latest_block = self
                    .read_provider
                    .get_block_by_number(BlockNumberOrTag::Latest)
                    .await
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?
                    .ok_or_else(|| SettlementError::Rpc("no latest block".into()))?;
                let base_fee = latest_block.header.base_fee_per_gas.ok_or_else(|| {
                    SettlementError::Rpc("chain does not support EIP-1559".into())
                })?;
                let tip = self
                    .read_provider
                    .get_max_priority_fee_per_gas()
                    .await
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;
                let fee_cap = u128::from(base_fee) * 2 + tip;

                let session_call = contract
                    .submitSession(
                        uuid_to_bytes32(params.session_id),
                        Bytes::copy_from_slice(&params.proof),
                        params.z_0,
                        params.z_n,
                        params.n_steps,
                        params.policy_hash,
                    )
                    .from(sender)
                    .nonce(nonce_a)
                    .gas(self.gas_limit)
                    .max_fee_per_gas(fee_cap)
                    .max_priority_fee_per_gas(tip)
                    .chain_id(self.chain_id);

                let session_pending = self
                    .submit_provider
                    .send_transaction(session_call.into_transaction_request())
                    .await
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;
                let session_tx_hash = *session_pending.tx_hash();
                let session_receipt = PendingTransactionBuilder::new(
                    self.read_provider.root().clone(),
                    session_tx_hash,
                )
                .get_receipt()
                .await
                .map_err(|e| SettlementError::Rpc(e.to_string()))?;
                session_receipt
                    .ensure_success()
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;

                let settle_nonce = nonce_a
                    .checked_add(1)
                    .ok_or_else(|| SettlementError::Rpc("operator nonce overflow".into()))?;

                // settleAuction: replay the matches array. The contract
                // recomputes the Poseidon settlement chain over it and reverts
                // unless it equals the proof's z_n[3] (#209).
                let settle_call = contract
                    .settleAuction(
                        uuid_to_bytes32(params.session_id),
                        uuid_to_bytes32(params.auction_id),
                        sol_matches,
                    )
                    .from(sender)
                    .nonce(settle_nonce)
                    .gas(self.gas_limit)
                    .max_fee_per_gas(fee_cap)
                    .max_priority_fee_per_gas(tip)
                    .chain_id(self.chain_id);

                let settle_pending = self
                    .submit_provider
                    .send_transaction(settle_call.into_transaction_request())
                    .await
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;
                let settle_tx_hash = *settle_pending.tx_hash();
                let settle_receipt = PendingTransactionBuilder::new(
                    self.read_provider.root().clone(),
                    settle_tx_hash,
                )
                .get_receipt()
                .await
                .map_err(|e| SettlementError::Rpc(e.to_string()))?;
                settle_receipt
                    .ensure_success()
                    .map_err(|e| SettlementError::Rpc(e.to_string()))?;

                Ok(format!("{:#x}", settle_receipt.transaction_hash))
            }
            .instrument(span),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SettlementMatch;
    use alloy_primitives::{address, U256};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    fn test_match() -> SettlementMatch {
        SettlementMatch {
            bid_order_id: Uuid::new_v4(),
            ask_order_id: Uuid::new_v4(),
            bid_trader: address!("0x0000000000000000000000000000000000000010"),
            ask_trader: address!("0x0000000000000000000000000000000000000020"),
            base_token: address!("0x0000000000000000000000000000000000000001"),
            quote_token: address!("0x0000000000000000000000000000000000000002"),
            price: Decimal::from(100),
            size: Decimal::from(10),
        }
    }

    fn test_params(matches: Vec<SettlementMatch>) -> SubmitBatchParams {
        SubmitBatchParams {
            batch_id: Uuid::new_v4(),
            auction_id: Uuid::new_v4(),
            proof: vec![0u8; GROTH16_PROOF_LEN],
            public_inputs: [U256::from(matches.len()); 6],
            matches,
        }
    }

    #[test]
    fn pack_submit_valid_selector() {
        let params = test_params(vec![test_match()]);
        let data = EthSubmitter::<alloy_provider::RootProvider>::pack_submit(&params).unwrap();
        assert!(data.len() > 4);
    }

    #[test]
    fn pack_submit_invalid_proof_length() {
        let mut params = test_params(vec![test_match()]);
        params.proof = vec![0u8; GROTH16_PROOF_LEN - 1];
        let err = EthSubmitter::<alloy_provider::RootProvider>::pack_submit(&params).unwrap_err();
        assert!(matches!(
            err,
            SettlementError::InvalidProofLength {
                expected: GROTH16_PROOF_LEN,
                actual
            } if actual == GROTH16_PROOF_LEN - 1
        ));
    }

    #[test]
    fn pack_submit_too_many_matches() {
        let matches: Vec<SettlementMatch> = (0..257).map(|_| test_match()).collect();
        let params = test_params(matches);
        let err = EthSubmitter::<alloy_provider::RootProvider>::pack_submit(&params).unwrap_err();
        assert!(matches!(
            err,
            SettlementError::TooManyMatches { count: 257 }
        ));
    }

    #[test]
    fn build_submit_span_records_batch_metadata() {
        // Span metadata is only kept around when a subscriber is
        // attached; install one for the duration of this test so we
        // can introspect the field set.
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(tracing_subscriber::fmt::layer());
        let _guard = tracing::subscriber::set_default(subscriber);

        let params = test_params(vec![test_match(), test_match()]);
        let span = build_submit_span(&params, SettlementTxTransport::PrivateRpc);
        let metadata = span.metadata().expect("subscriber attached, span enabled");
        // The span name is the operator/collector lookup key; assert
        // it explicitly so a rename surfaces here, not in a Jaeger UI
        // weeks later.
        assert_eq!(metadata.name(), "dp_settlement.eth_submit");
        let fields: Vec<&str> = metadata.fields().iter().map(|f| f.name()).collect();
        assert!(fields.contains(&"batch_id"), "fields: {fields:?}");
        assert!(fields.contains(&"auction_id"), "fields: {fields:?}");
        assert!(fields.contains(&"match_count"), "fields: {fields:?}");
        assert!(fields.contains(&"tx_transport"), "fields: {fields:?}");
    }
}
