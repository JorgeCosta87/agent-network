use anyhow::Result;
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    nonblocking::{
        pubsub_client::PubsubClient,
        rpc_client::RpcClient,
    },
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction
};
use tokio::sync::mpsc;
use std::sync::Arc;
use tokio_stream::StreamExt;
use base64::Engine;

use crate::types::AccountFilter;

pub struct SolanaAdapter {
    client: Arc<RpcClient>,
    ws_client: Arc<PubsubClient>,
    keypair: Keypair,
}

impl SolanaAdapter {
    pub async fn new(rpc_url: String, rpc_websocket_url: String, keypair: Keypair) -> Self {
        Self {
            client: Arc::new(
                RpcClient::new_with_commitment(
                    rpc_url,
                    CommitmentConfig::confirmed(),
                ),
            ),
            ws_client: Arc::new(match PubsubClient::new(&rpc_websocket_url).await {
                Ok(client) => client,
                Err(e) => {
                    panic!("Failed to create WebSocket client: {}", e);
                }
            }),
            keypair,
        }
    }

    pub fn payer_pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.client
            .get_balance(pubkey)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get balance: {}", e))
    }

    pub async fn send_and_confirm_transaction(
        &self,
        instructions: &[Instruction],
    ) -> Result<String> {
        let latest_blockhash = self.client
            .get_latest_blockhash()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get latest blockhash: {}", e))?;

        let payer = self.payer_pubkey();
        let transaction = Transaction::new_signed_with_payer(
            instructions,
            Some(&payer),
            &[&self.keypair],
            latest_blockhash,
        );

        self.client
            .send_and_confirm_transaction(&transaction)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send transaction: {}", e))?;

        Ok(transaction.signatures[0].to_string())
    }

    pub async fn get_program_accounts<T, F>(
        &self,
        program_id: &Pubkey,
        filters: Vec<AccountFilter>,
        deserialize_fn: F,
    ) -> Result<Vec<(Pubkey, T)>>
    where
        F: Fn(&[u8]) -> Result<T, std::io::Error> + Send + 'static,
        T: Send + 'static,
    {
        let mut rpc_filters = Vec::new();
        for filter in filters {
            rpc_filters.push(RpcFilterType::Memcmp(Memcmp::new(
                filter.offset,
                MemcmpEncodedBytes::Bytes(filter.value),
            )));
        }

        let config = RpcProgramAccountsConfig {
            filters: Some(rpc_filters),
            account_config: RpcAccountInfoConfig {
                commitment: Some(CommitmentConfig::confirmed()),
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: None,
                min_context_slot: None,
            },
            with_context: None,
            sort_results: None,
        };

        let accounts = self.client
            .get_program_ui_accounts_with_config(program_id, config)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get program accounts: {}", e))?;

        let mut decoded_accounts = Vec::new();
        for (pubkey, ui_account) in accounts {
            if let solana_account_decoder::UiAccountData::Binary(data_str, _) = &ui_account.data {
                if let Ok(decoded_bytes) = base64::engine::general_purpose::STANDARD.decode(data_str) {
                    match deserialize_fn(&decoded_bytes) {
                        Ok(data) => decoded_accounts.push((pubkey, data)),
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to deserialize account {}: {}",
                                pubkey,
                                e
                            ));
                        }
                    }
                }
            }
        }
        Ok(decoded_accounts)
    }

    pub async fn get_accounts<T, F>(
        &self,
        addresses: &[Pubkey],
        deserialize_fn: F,
    ) -> Result<Vec<(Pubkey, T)>>
    where
        F: Fn(&[u8]) -> Result<T, std::io::Error> + Send + 'static,
        T: Send + 'static,
    {
        let accounts = self.client
            .get_multiple_accounts(addresses)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get accounts: {}", e))?;

        let mut decoded_accounts = Vec::new();
        for i in 0..addresses.len() {
            let address = addresses[i];
            if let Some(account) = accounts[i].as_ref() {
                match deserialize_fn(&account.data) {
                    Ok(data) => decoded_accounts.push((address, data)),
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "Failed to deserialize account {}: {}",
                            address,
                            e
                        ));
                    }
                }
            }
        }
        Ok(decoded_accounts)
    }

    pub async fn get_account<T, F>(
        &self,
        address: &Pubkey,
        deserialize_fn: F,
    ) -> Result<Option<T>>
    where
        F: Fn(&[u8]) -> Result<T, std::io::Error> + Send + 'static,
        T: Send + 'static,
    {
        let accounts = self.get_accounts(&[*address], deserialize_fn).await?;
        Ok(accounts.into_iter().next().map(|(_, data)| data))
    }

    pub fn watch_program_accounts<T, F>(
        &self,
        program_id: &Pubkey,
        filters: Vec<AccountFilter>,
        deserialize_fn: F,
        tx: mpsc::Sender<T>,
        error_msg: &'static str,
    ) -> tokio::task::JoinHandle<()>
    where
        T: Send + 'static,
        F: Fn(&[u8]) -> Result<T, std::io::Error> + Send + 'static,
    {
        let ws_client = Arc::clone(&self.ws_client);
        let program_id = *program_id;

        let mut rpc_filters = Vec::new();
        for filter in filters {
            rpc_filters.push(RpcFilterType::Memcmp(Memcmp::new(
                filter.offset,
                MemcmpEncodedBytes::Bytes(filter.value),
            )));
        }

        let config = RpcProgramAccountsConfig {
            filters: Some(rpc_filters),
            account_config: RpcAccountInfoConfig {
                commitment: Some(CommitmentConfig::confirmed()),
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: None,
                min_context_slot: None,
            },
            with_context: None,
            sort_results: None,
        };

        tokio::spawn(async move {
            let (mut stream, _) = match ws_client
                .program_subscribe(&program_id, Some(config))
                .await
            {
                Ok(sub) => sub,
                Err(e) => {
                    panic!("Failed to subscribe to {}: {}", error_msg, e);
                }
            };

            while let Some(account) = stream.next().await {
                if let solana_account_decoder::UiAccountData::Binary(data_str, _) =
                    &account.value.account.data
                {
                    if let Ok(decoded_bytes) =
                        base64::engine::general_purpose::STANDARD.decode(data_str)
                    {
                        if let Ok(item) = deserialize_fn(&decoded_bytes) {
                            if tx.send(item).await.is_err() {
                                println!("Receiver dropped, stopping {} watch", error_msg);
                                break;
                            }
                        }
                    }
                }
            }
        })
    }
}
