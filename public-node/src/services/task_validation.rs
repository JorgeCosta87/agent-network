use std::sync::Arc;
use anyhow::{Result, Context};
use dac_sdk::{DacClient, types::TaskStatus};
use dac_common::{TaskInput, TaskOutput};
use ipfs_adapter::IpfsClient;
use solana_sdk::pubkey::Pubkey;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct TaskValidationService {
    dac_client: Arc<DacClient>,
    ipfs_client: Arc<IpfsClient>,
    validator_pubkey: Pubkey,
}

impl TaskValidationService {
    pub fn new(
        dac_client: Arc<DacClient>,
        ipfs_client: Arc<IpfsClient>,
        validator_pubkey: Pubkey,
    ) -> Self {
        Self {
            dac_client,
            ipfs_client,
            validator_pubkey,
        }
    }

    pub async fn run_task_validation_loop(&self, shutdown: CancellationToken) -> Result<()> {
        println!("Starting monitoring for tasks awaiting validation...");
        let (tx, mut rx) = mpsc::channel(1);
        let _subscription = self.dac_client.subscribe_to_tasks(
            Some(TaskStatus::AwaitingValidation),
            tx,
            shutdown.clone(),
        );

        println!("Monitoring for tasks to validate...");
        
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(task_account) => {
                            println!("Task awaiting validation: task_slot_id={}", task_account.task_slot_id);
                            
                            // Find goal for this task
                            let (goal, _) = match self.dac_client.get_goal_by_task_slot(task_account.task_slot_id).await {
                                Ok(result) => result,
                                Err(e) => {
                                    eprintln!("Could not find goal for task_slot_id={}: {}", task_account.task_slot_id, e);
                                    continue;
                                }
                            };
                            
                            // Only validate public goals (public nodes cannot validate confidential goals)
                            if goal.is_confidential {
                                println!("Skipping confidential goal (only confidential nodes can validate)");
                                continue;
                            }
                            
                            println!("Found public goal_id={} for task_slot_id={}", goal.goal_slot_id, task_account.task_slot_id);
                            if let Err(e) = self.validate_public_task(goal.goal_slot_id, task_account.task_slot_id).await {
                                eprintln!("Failed to validate task: {}", e);
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

    pub async fn validate_public_task(
        &self,
        goal_slot_id: u64,
        task_slot_id: u64,
    ) -> Result<String> {
        // Fetch task from chain
        let network_config = self.dac_client.derive_network_config_pda()?;
        let task = self.dac_client.get_task(&network_config, task_slot_id).await?
            .ok_or_else(|| anyhow::anyhow!("Task not found"))?;
        
        // Double-check task is still in AwaitingValidation status
        if task.status != TaskStatus::AwaitingValidation {
            return Err(anyhow::anyhow!("Task {} is no longer in AwaitingValidation status (current: {:?})", 
                task_slot_id, task.status));
        }
        
        // Fetch pending CIDs from task
        let pending_input_cid = task.pending_input_cid
            .ok_or_else(|| anyhow::anyhow!("No pending input CID (task may have been validated already)"))?;
        let pending_output_cid = task.pending_output_cid
            .ok_or_else(|| anyhow::anyhow!("No pending output CID (task may have been validated already)"))?;
        
        println!("Fetching task data from IPFS:");
        println!("  Input CID: {}", pending_input_cid);
        println!("  Output CID: {}", pending_output_cid);
        
        // Fetch input data from IPFS
        let input_bytes = self.ipfs_client.cat(&pending_input_cid).await
            .context("Failed to fetch input from IPFS")?;
        let _input_data: TaskInput = serde_json::from_slice(&input_bytes)
            .context("Failed to parse input data")?;
        
        // Fetch output data from IPFS
        let output_bytes = self.ipfs_client.cat(&pending_output_cid).await
            .context("Failed to fetch output from IPFS")?;
        let output_data: TaskOutput = serde_json::from_slice(&output_bytes)
            .context("Failed to parse output data")?;
        
        println!("Task data fetched successfully");
        println!("  LLM Response length: {} bytes", output_data.llm_response.len());
        
        // TODO: Add actual task execution validation logic (review output, check quality, etc.)
        // For now, we just validate that data exists and approve
        let approved = !output_data.llm_response.is_empty();
        
        // Determine goal completion: check if max_iterations is reached
        let goal = self.dac_client.get_goal(&network_config, goal_slot_id).await?
            .ok_or_else(|| anyhow::anyhow!("Goal not found"))?;
        
        let goal_completed = goal.current_iteration >= goal.max_iterations.saturating_sub(1);
        
        println!("Goal iteration status:");
        println!("  Current Iteration: {}", goal.current_iteration);
        println!("  Max Iterations: {}", goal.max_iterations);
        println!("  Iteration after validation: {}", goal.current_iteration + 1);
        println!("  Goal Completed: {}", goal_completed);
        
        // Calculate payment based on output size (mock logic)
        let payment_amount = 1_000_000; // 0.001 SOL
        
        // Submit public task validation (no TEE signature required)
        let submit_public_task_validation_signature = self.dac_client
            .submit_public_task_validation(
                &self.validator_pubkey,
                goal_slot_id,
                task_slot_id,
                payment_amount,
                approved,
                goal_completed,
            )
            .await?;

        println!("Submit Public Task Validation Signature: {}", submit_public_task_validation_signature);

        Ok(submit_public_task_validation_signature)
    }
}
