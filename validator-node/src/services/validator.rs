use anyhow::{Result, Context};
use dac_sdk::{SolanaAdapter, DacClient, types::{NodeType, NodeStatus, AgentStatus, TaskStatus}};
use ipfs_adapter::IpfsClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use crate::utils::{ValidateComputeNodeMessage, SubmitTaskValidationMessage, create_ed25519_instruction_with_signature};
use borsh::BorshSerialize;
use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};

use super::TeeService;

/// Task input data structure (fetched from IPFS)
#[derive(Debug, Serialize, Deserialize)]
struct TaskInputData {
    goal_specification: String,
    agent_config: String,
    agent_memory: Option<String>,
    previous_output: Option<String>,
}

/// Task output data structure (fetched from IPFS)
#[derive(Debug, Serialize, Deserialize)]
struct TaskOutputData {
    llm_response: String,
    reasoning: Option<String>,
    next_action: Option<String>,
}

/// Task execution snapshot (uploaded to IPFS by validator)
#[derive(Debug, Serialize, Deserialize)]
struct TaskExecutionSnapshot {
    task_slot_id: u64,
    execution_count: u64,
    goal_id: u64,
    agent_pubkey: String,
    compute_node: Option<String>,
    input_cid: Option<String>,
    output_cid: Option<String>,
    timestamp: i64,
    chain_proof: String,
    validation: ValidationInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct ValidationInfo {
    validator_pubkey: String,
    tee_signature: String,
    payment_amount: u64,
    approved: bool,
}


pub struct ValidatorService {
    dac_client: DacClient,
    ipfs_client: Arc<IpfsClient>,
    tee_service: Arc<TeeService>,
    validator_pubkey: Pubkey,
}

impl ValidatorService {
    pub fn new(
        adapter: Arc<SolanaAdapter>,
        ipfs_client: Arc<IpfsClient>,
        tee_service: Arc<TeeService>,
        authority: Pubkey,
        validator_pubkey: Pubkey,
    ) -> Self {
        let dac_client = DacClient::new(adapter, authority);
        Self {
            dac_client,
            ipfs_client,
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
            if let Err(e) = self.validate_compute_node(&node_info.node_pubkey).await {
                eprintln!("Failed to validate compute node: {}", e);
            }
        }
        Ok(())
    }

    pub async fn validate_agents_with_pending_status(&self) -> Result<()> {
        let agents = self.dac_client.get_all_agents_by_status(AgentStatus::Pending).await?;
        for agent in agents {
            println!("Agent awaiting validation: slot_id={}", agent.agent_slot_id);
            self.validate_agent(&self.validator_pubkey, agent.agent_slot_id).await?;
        }
        Ok(())
    }

    pub async fn run_validation_loop(&self, shutdown: CancellationToken) -> Result<()> {
        // First, validate any nodes that are already awaiting validation
        println!("Validating existing compute nodes awaiting validation...");
        self.validate_compute_nodes_awaiting_validation().await?;
        
        // Setup subscription for new compute nodes
        println!("Starting monitoring for new compute nodes awaiting validation...");
        let (tx, mut rx) = mpsc::channel(1);
        let _subscription = self.dac_client.subscribe_to_node_info(
            None,
            Some(NodeType::Compute),
            Some(NodeStatus::AwaitingValidation),
            tx,
        )?;

        println!("Monitoring for compute nodes to validate...");
        
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
                                }
                            }
                        }
                        None => {
                            //TODO: try to retry the subscription
                            anyhow::bail!("Compute node subscription ended unexpectedly");
                        }
                    }
                }
                _ = shutdown.cancelled() => {
                    println!("Compute node validation loop shutting down...");
                    return Ok(());
                }
            }
        }
    }

    pub async fn run_agent_validation_loop(&self, shutdown: CancellationToken) -> Result<()> {
        println!("Validating existing agents with pending status...");
        self.validate_agents_with_pending_status().await?;
        
        println!("Starting monitoring for new agents with pending status...");
        let (tx, mut rx) = mpsc::channel(1);
        let subscription = self.dac_client.subscribe_to_agents(
            Some(AgentStatus::Pending),
            tx,
        );

        println!("Monitoring for agents to validate...");
        
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(agent) => {
                            println!("Agent awaiting validation: slot_id={}", agent.agent_slot_id);
                            match self.validate_agent(&self.validator_pubkey, agent.agent_slot_id).await {
                                Ok(sig) => {
                                    println!("Successfully validated agent. Transaction: {:?}", sig);
                                }
                                Err(e) => {
                                    eprintln!("Failed to validate agent: {}", e);
                                }
                            }
                        }
                        None => {
                            anyhow::bail!("Agent subscription ended unexpectedly");
                        }
                    }
                }
                _ = shutdown.cancelled() => {
                    println!("Agent validation loop shutting down...");
                    drop(subscription);
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

    //TODO: Add actual task execution validation logic (fetch from IPFS, re-execute, etc.)
    pub async fn validate_task(
        &self,
        goal_slot_id: u64,
        task_slot_id: u64,
    ) -> Result<String> {
        let tee_signing_keypair = self.tee_service.get_signing_keypair()?;
        
        // Fetch task from chain
        let network_config = self.dac_client.derive_network_config_pda()?;
        let task = self.dac_client.get_task(&network_config, task_slot_id).await?
            .ok_or_else(|| anyhow::anyhow!("Task not found"))?;
        
        // Fetch pending CIDs from task
        let pending_input_cid = task.pending_input_cid
            .ok_or_else(|| anyhow::anyhow!("No pending input CID"))?;
        let pending_output_cid = task.pending_output_cid
            .ok_or_else(|| anyhow::anyhow!("No pending output CID"))?;
        
        println!("Fetching task data from IPFS:");
        println!("  Input CID: {}", pending_input_cid);
        println!("  Output CID: {}", pending_output_cid);
        
        // Fetch input data from IPFS
        let input_bytes = self.ipfs_client.cat(&pending_input_cid).await
            .context("Failed to fetch input from IPFS")?;
        let input_data: TaskInputData = serde_json::from_slice(&input_bytes)
            .context("Failed to parse input data")?;
        
        // Fetch output data from IPFS
        let output_bytes = self.ipfs_client.cat(&pending_output_cid).await
            .context("Failed to fetch output from IPFS")?;
        let output_data: TaskOutputData = serde_json::from_slice(&output_bytes)
            .context("Failed to parse output data")?;
        
        println!("Task data fetched successfully");
        println!("  LLM Response length: {} bytes", output_data.llm_response.len());
        
        // TODO: Re-execute task in TEE for validation
        // For now, we just validate that data exists and approve
        let approved = !output_data.llm_response.is_empty();
        
        // TODO: Determine goal completion by analyzing output
        let goal_completed = false;
        
        // Calculate payment based on output size (mock logic)
        let payment_amount = 1_000_000; // 0.001 SOL
        
        // Compute validation_proof = SHA256(pending_input_cid + pending_output_cid)
        let mut hasher = Sha256::new();
        hasher.update(pending_input_cid.as_bytes());
        hasher.update(pending_output_cid.as_bytes());
        let validation_proof: [u8; 32] = hasher.finalize().into();
        
        // Create message
        let message = SubmitTaskValidationMessage {
            goal_id: goal_slot_id,
            task_slot_id,
            payment_amount,
            validation_proof,
            approved,
            goal_completed,
        };
        
        // Serialize message
        let mut message_data = Vec::new();
        message
            .serialize(&mut message_data)
            .map_err(|e| anyhow::anyhow!("Failed to serialize task validation message: {}", e))?;
        
        // Create Ed25519 instruction
        let ed25519_instruction = create_ed25519_instruction_with_signature(
            &message_data,
            &tee_signing_keypair,
        );
        
        // Create and upload snapshot to IPFS
        let snapshot = TaskExecutionSnapshot {
            task_slot_id,
            execution_count: task.execution_count,
            goal_id: goal_slot_id,
            agent_pubkey: task.agent.to_string(),
            compute_node: task.compute_node.map(|n| n.to_string()),
            input_cid: Some(pending_input_cid.clone()),
            output_cid: Some(pending_output_cid.clone()),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            chain_proof: hex::encode(task.chain_proof),
            validation: ValidationInfo {
                validator_pubkey: self.validator_pubkey.to_string(),
                tee_signature: hex::encode(&message_data[..32]), // Mock TEE signature
                payment_amount,
                approved,
            },
        };
        
        let snapshot_json = serde_json::to_vec(&snapshot)
            .context("Failed to serialize snapshot")?;
        
        let snapshot_cid = self.ipfs_client.add(snapshot_json).await
            .context("Failed to upload snapshot to IPFS")?;
        
        println!("Snapshot uploaded to IPFS: {}", snapshot_cid);
        
        self.ipfs_client.pin(&snapshot_cid).await
            .context("Failed to pin snapshot")?;
        
        // Submit with Ed25519 instruction
        self.dac_client
            .submit_task_validation(
                &self.validator_pubkey,
                goal_slot_id,
                task_slot_id,
                ed25519_instruction,
            )
            .await
    }

    pub async fn run_task_validation_loop(&self, shutdown: CancellationToken) -> Result<()> {
        println!("Starting monitoring for tasks awaiting validation...");
        let (tx, mut rx) = mpsc::channel(1);
        let _subscription = self.dac_client.subscribe_to_tasks(
            Some(TaskStatus::AwaitingValidation),
            tx,
        );

        println!("Monitoring for tasks to validate...");
        
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(task_account) => {
                            println!("Task awaiting validation: task_slot_id={}", task_account.task_slot_id);
                            
                            let network_config = self.dac_client.derive_network_config_pda()?;
                            let network_config_data = self.dac_client.get_network_config().await?
                                .ok_or_else(|| anyhow::anyhow!("Network config not found"))?;
                            
                            let task_pubkey = self.dac_client.derive_task_pda(&network_config, task_account.task_slot_id)?;
                            
                            let mut found_goal_id = None;
                            for goal_id in 0..network_config_data.goal_count {
                                if let Ok(Some(goal)) = self.dac_client.get_goal(&network_config, goal_id).await {
                                    if goal.task == task_pubkey {
                                        found_goal_id = Some(goal_id);
                                        break;
                                    }
                                }
                            }
                            
                            if let Some(goal_slot_id) = found_goal_id {
                                println!("Found goal_id={} for task_slot_id={}", goal_slot_id, task_account.task_slot_id);
                                if let Err(e) = self.validate_task(goal_slot_id, task_account.task_slot_id).await {
                                    eprintln!("Failed to validate task: {}", e);
                                }
                            } else {
                                eprintln!("Could not find goal for task_slot_id={}", task_account.task_slot_id);
                            }
                        }
                        None => {
                            anyhow::bail!("Task subscription ended unexpectedly");
                        }
                    }
                }
                _ = shutdown.cancelled() => {
                    println!("Task validation loop shutting down...");
                    return Ok(());
                }
            }
        }
    }
}
