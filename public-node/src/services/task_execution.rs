use std::sync::Arc;
use anyhow::Result;
use dac_sdk::{DacClient, types::TaskStatus};
use dac_client::types::ActionType as SdkActionType;
use dac_common::{TaskInput, TaskOutput, ActionType};
use ipfs_adapter::IpfsClient;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct TaskExecutionService {
    dac_client: Arc<DacClient>,
    ipfs_adapter: Arc<IpfsClient>,
}

impl TaskExecutionService {
    pub fn new(
        dac_client: Arc<DacClient>,
        ipfs_adapter: Arc<IpfsClient>,
    ) -> Self {
        Self {
            dac_client,
            ipfs_adapter,
        }
    }

    /// Claim a task for execution
    pub async fn claim_task(&self, task_slot_id: u64, max_task_cost: u64) -> Result<()> {
        println!("Claiming task {}", task_slot_id);
        
        // Get task account to check input_cid
        let task_account = self.dac_client.get_task_by_slot(task_slot_id).await?;
        
        let has_input = task_account.input_cid.is_some();
        
        if has_input {
            println!("Task has previous input CID: {:?}", task_account.input_cid);
        } else {
            println!("Task is first iteration (no input CID)");
        }
        
        // Submit claim transaction
        self.dac_client.claim_task(task_slot_id, max_task_cost).await?;
        
        println!("Task {} claimed successfully", task_slot_id);
        Ok(())
    }

    pub async fn execute_task(&self, task_slot_id: u64) -> Result<()> {
        println!("Executing task {}", task_slot_id);
        
        let task_account = self.dac_client.get_task_by_slot(task_slot_id).await?;
        
        let (goal_account, goal_address) = self.dac_client.get_goal_by_task_slot(task_slot_id).await?;
        println!("Found goal at address: {} for task {}", goal_address, task_slot_id);

        let agent_account = self.dac_client.get_agent_by_pubkey(goal_account.agent).await?;
        
        // 2. Build combined context string
        let goal_spec = String::from_utf8(self.ipfs_adapter.cat(&goal_account.specification_cid).await?.to_vec())?;
        let agent_config = String::from_utf8(self.ipfs_adapter.cat(&agent_account.agent_config_cid).await?.to_vec())?;
        
        let agent_memory = if let Some(ref memory_cid) = agent_account.agent_memory_cid {
            Some(String::from_utf8(self.ipfs_adapter.cat(memory_cid).await?.to_vec())?)
        } else {
            None
        };
        
        // Build combined context string
        let mut context_parts = vec![
            format!("Goal Specification:\n{}", goal_spec),
            format!("Agent Config:\n{}", agent_config),
        ];
        
        // Add iteration information so agent knows when to finish
        context_parts.push(format!(
            "Iteration Information:\n  Current Iteration: {}\n  Max Iterations: {}\n  Remaining Iterations: {}",
            goal_account.current_iteration,
            goal_account.max_iterations,
            goal_account.max_iterations.saturating_sub(goal_account.current_iteration)
        ));
        
        if let Some(ref memory) = agent_memory {
            context_parts.push(format!("Agent Memory:\n{}", memory));
        }
        
        // Add previous output if exists (from previous iteration's output_cid)
        if let Some(ref output_cid) = task_account.output_cid {
            let previous_output = String::from_utf8(self.ipfs_adapter.cat(output_cid).await?.to_vec())?;
            context_parts.push(format!("Previous Output:\n{}", previous_output));
        }
        
        let context = context_parts.join("\n\n---\n\n");
        
        // Convert ActionType from Task to dac_common::ActionType
        let action_type = match task_account.action_type {
            SdkActionType::Llm => ActionType::Llm,
            SdkActionType::Tool(pubkey) => ActionType::Tool { pubkey: pubkey.to_string() },
            SdkActionType::Agent2Agent(pubkey) => ActionType::Agent2Agent { pubkey: pubkey.to_string() },
        };
        
        let input_context = TaskInput {
            context,
            action_type,
        };
        
        // 4. Upload current input context to IPFS (for tracking what was used)
        let input_json = serde_json::to_string(&input_context)?;
        let input_cid = self.ipfs_adapter.add(input_json.as_bytes().to_vec()).await?;
        self.ipfs_adapter.pin(&input_cid).await?;
        println!("Input CID: {}", input_cid);   

        // 5. Execute with LLM (placeholder - will be implemented with actual LLM)
        let output = self.execute_llm(&input_context).await?;
        
        // 6. Upload output to IPFS
        let output_json = serde_json::to_string(&output)?;
        let output_cid = self.ipfs_adapter.add(output_json.as_bytes().to_vec()).await?;
        self.ipfs_adapter.pin(&output_cid).await?;
        println!("Output CID: {}", output_cid);
        
        // 7. Prepare input for next iteration (current context + output)
        let next_context = format!("{}\n\n---\n\nPrevious Output:\n{}", 
            input_context.context, 
            output_json
        );
        
        let next_input = TaskInput {
            context: next_context,
            action_type: input_context.action_type,
        };
        
        // 8. Upload next input to IPFS
        let next_input_json = serde_json::to_string(&next_input)?;
        let next_input_cid = self.ipfs_adapter.add(next_input_json.as_bytes().to_vec()).await?;
        self.ipfs_adapter.pin(&next_input_cid).await?;
        
        // 9. Submit result to on-chain (input_cid, output_cid, next_input_cid)
        let submit_task_result_signature = self.dac_client.submit_task_result(
            task_slot_id,
            goal_account.goal_slot_id,
            input_cid.clone(),
            output_cid.clone(),
            next_input_cid.clone(),
        ).await?;
        
        println!("Task {} executed successfully", task_slot_id);
        println!("  Input CID:      {}", input_cid);
        println!("  Output CID:     {}", output_cid);
        println!("  Next Input CID: {}", next_input_cid);
        println!("  Submit Task Result Signature: {}", submit_task_result_signature);

        let task_account_after = self.dac_client.get_task_by_slot(task_slot_id).await?;
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Task Execution Summary");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        println!("  Task Slot ID:     {}", task_slot_id);
        println!("  Goal Slot ID:     {}", goal_account.goal_slot_id);
        println!("  Input CID:      {:?}", task_account_after.input_cid);
        println!("  Output CID:     {:?}", task_account_after.output_cid);
        println!("  Next Input CID: {:?}", task_account_after.next_input_cid);
        println!("  Status:           {:?}", task_account_after.status);

        Ok(())
    }

    /// Execute LLM (placeholder implementation)
    async fn execute_llm(&self, input: &TaskInput) -> Result<TaskOutput> {
        // TODO: Implement actual LLM execution
        // For now, return a placeholder response
        
        println!("Executing LLM with input:");
        println!("  Action Type: {:?}", input.action_type);
        println!("  Context: {} chars", input.context.len());
        
        // Placeholder response
        let output = TaskOutput {
            llm_response: "Placeholder LLM response".to_string(),
            reasoning: "Placeholder reasoning".to_string(),
        };
        
        Ok(output)
    }

    /// Run task execution loop with shutdown token
    pub async fn run_task_execution_loop(&self, shutdown: CancellationToken) -> Result<()> {
        println!("Starting task execution monitoring...");
        
        // First, check for existing pending tasks
        println!("Checking for existing pending tasks...");
        self.execute_pending_tasks().await?;
        
        // Setup subscription for new pending tasks
        println!("Starting monitoring for new pending tasks...");
        let (tx, mut rx) = mpsc::channel(1);
        let _subscription = self.dac_client.subscribe_to_tasks(
            Some(TaskStatus::Pending),
            tx,
            shutdown.clone(),
        );

        println!("Monitoring for tasks to claim and execute...");
        
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(task_account) => {
                            println!("Pending task detected: task_slot_id={}", task_account.task_slot_id);
                            println!("  Status:           {:?}", task_account.status);
                            
                            // Claim and execute the task
                            if let Err(e) = self.claim_and_execute_task(task_account.task_slot_id).await {
                                eprintln!("Failed to claim/execute task {}: {}", task_account.task_slot_id, e);
                            }
                        }
                        None => {
                            anyhow::bail!("Task subscription ended unexpectedly");
                        }
                    }
                }
                _ = shutdown.cancelled() => {
                    println!("Task execution loop shutdown requested");
                    return Ok(());
                }
            }
        }
    }

    /// Execute any existing pending tasks
    async fn execute_pending_tasks(&self) -> Result<()> {
        let network_config = self.dac_client.derive_network_config_pda()?;
        let network_config_data = self.dac_client.get_network_config().await?
            .ok_or_else(|| anyhow::anyhow!("Network config not found"))?;
        
        for task_slot_id in 0..network_config_data.task_count {
            if let Ok(Some(task)) = self.dac_client.get_task(&network_config, task_slot_id).await {
                if matches!(task.status, TaskStatus::Pending) {
                    println!("Found existing pending task: task_slot_id={}", task_slot_id);
                    if let Err(e) = self.claim_and_execute_task(task_slot_id).await {
                        eprintln!("Failed to claim/execute task {}: {}", task_slot_id, e);
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Claim and execute a task
    async fn claim_and_execute_task(&self, task_slot_id: u64) -> Result<()> {
        println!("Claiming and executing task {}", task_slot_id);
        
        // TODO: Make max_task_costcalculate based on task complexity
        let max_task_cost = 1_000;
        self.claim_task(task_slot_id, max_task_cost).await?;
        
        // Execute the task
        self.execute_task(task_slot_id).await?;
        
        println!("Successfully claimed and executed task {}", task_slot_id);
        Ok(())
    }
}
