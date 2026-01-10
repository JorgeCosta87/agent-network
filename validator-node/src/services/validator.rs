use anyhow::Result;
use dac_sdk::{SolanaAdapter, DacClient, types::{NodeType, NodeStatus}};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::utils::{ValidateComputeNodeMessage, create_ed25519_instruction_with_signature};
use borsh::BorshSerialize;

use super::TeeService;

pub struct ValidatorService {
    dac_client: DacClient,
    tee_service: Arc<TeeService>,
    validator_pubkey: Pubkey,
}

impl ValidatorService {
    pub fn new(adapter: Arc<SolanaAdapter>, tee_service: Arc<TeeService>, authority: Pubkey, validator_pubkey: Pubkey) -> Self {
        let dac_client = DacClient::new(adapter, authority);
        Self {
            dac_client,
            tee_service,
            validator_pubkey,
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
        let claimed = self.monitor_validator_node_account(node_pubkey, code_measurement, tee_signing_pubkey).await?;
        
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

    pub async fn monitor_validator_node_account(
        &self,
        node_pubkey: &Pubkey,
        code_measurement: [u8; 32],
        tee_signing_pubkey: Pubkey,
    ) -> Result<bool> {
        let (tx, mut rx) = mpsc::channel(1);
        let handle = self.dac_client.subscribe_to_node_info(
            Some(node_pubkey),
            Some(NodeType::Validator),
            Some(NodeStatus::PendingClaim),
            tx,
        )?;
            
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(node_info) => {
                            println!("Validator node info received: status={:?}", node_info.status);
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
                                    anyhow::bail!("Failed to claim validator node: {}", e);
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


    pub async fn validate_compute_node(
        &self,
        compute_node_pubkey: &Pubkey,
    ) -> Result<String> {
        let tee_signing_keypair = self.tee_service.get_signing_keypair()?;

        let message = ValidateComputeNodeMessage {
            compute_node_pubkey: *compute_node_pubkey,
            approved: true, //TODO: Add approval logic
        };

        let mut message_data = Vec::new();
        message
            .serialize(&mut message_data)
            .map_err(|e| anyhow::anyhow!("Failed to serialize validation message: {}", e))?;

        let ed25519_instruction = create_ed25519_instruction_with_signature(
            &message_data,
            &tee_signing_keypair,
        );

        self.dac_client
            .validate_compute_node(&self.validator_pubkey, compute_node_pubkey, ed25519_instruction)
            .await
    }

    pub async fn validate_compute_nodes_awaiting_validation(
        &self,
    ) -> Result<()> {
        let nodes = self.dac_client.get_all_compute_node_by_status_and_type(
                NodeStatus::AwaitingValidation, NodeType::Compute).await?;
        for node_info in nodes {
            println!("Compute node awaiting validation: {:?}", node_info.node_pubkey);
            self.validate_compute_node(&node_info.node_pubkey).await?;
        }
        Ok(())
    }

    pub async fn run_validation_loop(&self) -> Result<()> {
        // First, validate any nodes that are already awaiting validation
        println!("Validating existing compute nodes awaiting validation...");
        self.validate_compute_nodes_awaiting_validation().await?;
        
        // Setup subscription for new compute nodes
        println!("Starting monitoring for new compute nodes awaiting validation...");
        let (tx, mut rx) = mpsc::channel(1);
        let handle = self.dac_client.subscribe_to_node_info(
            None,
            Some(NodeType::Compute),
            Some(NodeStatus::AwaitingValidation),
            tx,
        )?;

        println!("Validator node ready. Monitoring for compute nodes to validate...");
        
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(node_info) => {
                            println!("Compute node awaiting validation: {:?}", node_info.node_pubkey);
                            match self.validate_compute_node(&node_info.node_pubkey).await {
                                Ok(sig) => {
                                    println!("Successfully validated compute node. Transaction: {:?}", sig);
                                }
                                Err(e) => {
                                    eprintln!("Failed to validate compute node: {}", e);
                                    anyhow::bail!("Validation failed: {}", e);
                                }
                            }
                        }
                        None => {
                            println!("Subscription channel closed unexpectedly");
                            handle.abort();
                            anyhow::bail!("Subscription ended unexpectedly");
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("Shutdown signal received. Cleaning up...");
                    handle.abort();
                    return Ok(());
                }
            }
        }
    }

    /// TODO: Add config validation and model availability checks
    pub async fn validate_agent(
        &self,
        validator_pubkey: &Pubkey,
        agent_slot_id: u64,
    ) -> Result<String> {
        self.dac_client
            .validate_agent(validator_pubkey, agent_slot_id)
            .await
    }
}
