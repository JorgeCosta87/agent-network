use anyhow::Result;
use dac_sdk::{SolanaAdapter, DacClient, types::{NodeType, NodeStatus}};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::TeeService;

pub struct ValidatorService {
    dac_client: DacClient,
    tee_service: Arc<TeeService>,
}

impl ValidatorService {
    pub fn new(adapter: Arc<SolanaAdapter>, tee_service: Arc<TeeService>) -> Self {
        Self {
            dac_client: DacClient::new(adapter),
            tee_service,
        }
    }

    pub async fn handle_validator_claim(&self, node_pubkey: &Pubkey) -> Result<bool> {
        // Get mocked TEE attestation data
        let code_measurement = self.tee_service.get_code_measurement()?;
        let tee_signing_pubkey = self.tee_service.get_signing_pubkey()?;

        if self.try_claim_validator(node_pubkey, code_measurement, tee_signing_pubkey).await? {
            println!("Validator node claimed successfully.");
            return Ok(true);
        }
        
        // Node not registered yet, monitor until we can claim it
        println!("Node not registered. Monitoring for node registration...");
        let claimed = self.monitor_validator_account(node_pubkey, code_measurement, tee_signing_pubkey).await?;
        
        if claimed {
            println!("Validator node claimed successfully after monitoring.");
            Ok(true)
        } else {
            println!("Node monitoring interrupted. Node not claimed.");
            Ok(false)
        }
    }

    pub async fn try_claim_validator(
        &self,
        node_pubkey: &Pubkey,
        code_measurement: [u8; 32],
        tee_signing_pubkey: Pubkey,
    ) -> Result<bool> {
        match self.dac_client.get_node_info(node_pubkey).await? {
            Some(node_info) => {
                match node_info.status {
                    NodeStatus::PendingClaim => {
                        println!("Node registered but not claimed. Claiming validator node with TEE attestation...");
                        let tx = self.dac_client.claim_validator_node(
                            node_pubkey,
                            code_measurement,
                            tee_signing_pubkey,
                        ).await?;
                        println!("Transaction: {:?}", tx);
                        Ok(true)
                    }
                    NodeStatus::Active => {
                        println!("Validator node already active. Status: {:?}", node_info.status);
                        Ok(true)
                    }
                    _ => {
                        println!("Node status: {:?}", node_info.status);
                        Ok(false)
                    }
                }
            }
            None => {
                println!("Node not registered.");
                Ok(false)
            }
        }
    }

    pub async fn monitor_validator_account(
        &self,
        node_pubkey: &Pubkey,
        code_measurement: [u8; 32],
        tee_signing_pubkey: Pubkey,
    ) -> Result<bool> {
        let (tx, mut rx) = mpsc::channel(1);
        let handle = self.dac_client.subscribe_to_node_info(node_pubkey, Some(NodeStatus::PendingClaim), tx)?;

        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(node_info) => {
                            println!("Validator node info received: status={:?}", node_info.status);

                            if node_info.status == NodeStatus::PendingClaim && node_info.node_type == NodeType::Validator {
                                match self.dac_client.claim_validator_node(
                                    node_pubkey,
                                    code_measurement,
                                    tee_signing_pubkey,
                                ).await {
                                    Ok(sig) => {
                                        println!("Successfully claimed validator node. Transaction: {:?}", sig);
                                        handle.abort();
                                        return Ok(true);
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to claim validator node: {}", e);
                                        anyhow::bail!(e);
                                    }
                                }
                            }
                        }
                        None => {
                            println!("Watch channel closed");
                            handle.abort();
                            return Ok(false);
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("Node monitoring interrupted by user");
                    handle.abort();
                    return Ok(false);
                }
            }
        }
    }
}
