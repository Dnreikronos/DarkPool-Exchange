use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

use alloy_network::{Ethereum, EthereumWallet, NetworkWallet, TransactionBuilder};
use alloy_primitives::{Address, Bytes};
use alloy_provider::Provider;
use alloy_rpc_types::{BlockNumberOrTag, TransactionRequest};
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolCall;

use crate::abi::{submitBatchCall, Match as SolMatch, MAX_MATCHES_PER_BATCH};
use crate::helpers::{decimal_to_wei, uuid_to_bytes32};
use crate::submitter::Submitter;
use crate::{SettlementError, SettlementMatch, SubmitBatchParams};

pub struct EthSubmitterConfig {
    pub rpc_url: String,
    pub private_key: String,
    pub contract_address: String,
    pub chain_id: u64,
    pub gas_limit: Option<u64>,
}

pub struct EthSubmitter<P> {
    provider: P,
    wallet: EthereumWallet,
    contract: Address,
    chain_id: u64,
    gas_limit: u64,
}

impl<P: Provider + Send + Sync> EthSubmitter<P> {
    pub fn new(provider: P, config: &EthSubmitterConfig) -> Result<Self, SettlementError> {
        let signer = PrivateKeySigner::from_str(&config.private_key)
            .map_err(|e| SettlementError::Signer(e.to_string()))?;
        let wallet = EthereumWallet::from(signer);
        let contract = Address::from_str(&config.contract_address)
            .map_err(|e| SettlementError::Rpc(e.to_string()))?;
        Ok(Self {
            provider,
            wallet,
            contract,
            chain_id: config.chain_id,
            gas_limit: config.gas_limit.unwrap_or(500_000),
        })
    }

    pub fn pack_submit(params: &SubmitBatchParams) -> Result<Vec<u8>, SettlementError> {
        if params.matches.len() > MAX_MATCHES_PER_BATCH {
            return Err(SettlementError::TooManyMatches {
                count: params.matches.len(),
            });
        }

        let sol_matches: Vec<SolMatch> = params
            .matches
            .iter()
            .map(settlement_match_to_sol)
            .collect::<Result<_, SettlementError>>()?;

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

fn settlement_match_to_sol(m: &SettlementMatch) -> Result<SolMatch, SettlementError> {
    Ok(SolMatch {
        bidOrderId: uuid_to_bytes32(m.bid_order_id),
        askOrderId: uuid_to_bytes32(m.ask_order_id),
        bidTrader: m.bid_trader,
        askTrader: m.ask_trader,
        baseToken: m.base_token,
        quoteToken: m.quote_token,
        price: decimal_to_wei(m.price)?,
        size: decimal_to_wei(m.size)?,
    })
}

impl<P: Provider + Send + Sync + 'static> Submitter for EthSubmitter<P> {
    fn submit<'a>(
        &'a self,
        params: &'a SubmitBatchParams,
    ) -> Pin<Box<dyn Future<Output = Result<String, SettlementError>> + Send + 'a>> {
        Box::pin(async move {
            let calldata = Self::pack_submit(params)?;

            let nonce = self
                .provider
                .get_transaction_count(NetworkWallet::<Ethereum>::default_signer_address(
                    &self.wallet,
                ))
                .await
                .map_err(|e| SettlementError::Rpc(e.to_string()))?;

            let latest_block = self
                .provider
                .get_block_by_number(BlockNumberOrTag::Latest)
                .await
                .map_err(|e| SettlementError::Rpc(e.to_string()))?
                .ok_or_else(|| SettlementError::Rpc("no latest block".into()))?;

            let base_fee = latest_block
                .header
                .base_fee_per_gas
                .ok_or_else(|| SettlementError::Rpc("chain does not support EIP-1559".into()))?;

            let tip = self
                .provider
                .get_max_priority_fee_per_gas()
                .await
                .map_err(|e| SettlementError::Rpc(e.to_string()))?;
            let fee_cap = u128::from(base_fee) * 2 + tip;

            let sender = NetworkWallet::<Ethereum>::default_signer_address(&self.wallet);
            let mut tx = TransactionRequest::default()
                .from(sender)
                .to(self.contract)
                .input(calldata.into())
                .nonce(nonce)
                .gas_limit(self.gas_limit)
                .max_fee_per_gas(fee_cap)
                .max_priority_fee_per_gas(tip);
            tx.set_chain_id(self.chain_id);

            let receipt = self
                .provider
                .send_transaction(tx)
                .await
                .map_err(|e| SettlementError::Rpc(e.to_string()))?
                .get_receipt()
                .await
                .map_err(|e| SettlementError::Rpc(e.to_string()))?;

            Ok(format!("{:#x}", receipt.transaction_hash))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            proof: b"proof".to_vec(),
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
    fn pack_submit_too_many_matches() {
        let matches: Vec<SettlementMatch> = (0..257).map(|_| test_match()).collect();
        let params = test_params(matches);
        let err = EthSubmitter::<alloy_provider::RootProvider>::pack_submit(&params).unwrap_err();
        assert!(matches!(
            err,
            SettlementError::TooManyMatches { count: 257 }
        ));
    }
}
