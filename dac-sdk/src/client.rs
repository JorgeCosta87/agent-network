use anyhow::Result;
use solana_adapter::{SolanaAdapter, AccountFilter};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::mpsc;

use dac_client::{
    accounts::{
        Agent, NodeInfo, Task, Goal, NetworkConfig, Contribution,
        AGENT_DISCRIMINATOR, NODE_INFO_DISCRIMINATOR, TASK_DISCRIMINATOR,
        GOAL_DISCRIMINATOR,
    },
    instructions::{
        ClaimPublicNodeBuilder, ClaimConfidentialNodeBuilder, RegisterNodeBuilder, 
        SubmitTaskResultBuilder, ClaimTaskBuilder, CreateAgentBuilder, CreateGoalBuilder,
        SubmitPublicTaskValidationBuilder, SubmitConfidentialTaskValidationBuilder, ValidateAgentBuilder,
        ValidatePublicNodeBuilder, InitializeNetworkBuilder,
        UpdateNetworkConfigBuilder,
    },
    types::{NodeType, TaskStatus, AgentStatus, GoalStatus, NodeStatus},
    programs::DAC_ID,
};

use crate::pdas;

pub struct DacClient {
    adapter: Arc<SolanaAdapter>,
    authority: Pubkey,
}

impl DacClient {
    pub fn new(adapter: Arc<SolanaAdapter>, authority: Pubkey) -> Self {
        Self { 
            adapter,
            authority,
        }
    }

    fn get_authority(&self) -> Result<&Pubkey> {
        Ok(&self.authority)
    }

    pub fn derive_network_config_pda(&self) -> Result<Pubkey> {
        let authority = self.get_authority()?;
        Ok(pdas::derive_network_config_pda(&authority)?.0)
    }

    pub fn derive_node_info_pda(&self, node_pubkey: &Pubkey) -> Result<Pubkey> {
        Ok(pdas::derive_node_info_pda(node_pubkey)?.0)
    }

    pub fn derive_agent_pda(&self, network_config: &Pubkey, agent_slot_id: u64) -> Result<Pubkey> {
        Ok(pdas::derive_agent_pda(network_config, agent_slot_id)?.0)
    }

    pub fn derive_goal_pda(&self, network_config: &Pubkey, goal_slot_id: u64) -> Result<Pubkey> {
        Ok(pdas::derive_goal_pda(network_config, goal_slot_id)?.0)
    }

    pub fn derive_task_pda(&self, network_config: &Pubkey, task_slot_id: u64) -> Result<Pubkey> {
        Ok(pdas::derive_task_pda(network_config, task_slot_id)?.0)
    }

    pub fn derive_node_treasury_pda(&self, node_info: &Pubkey) -> Result<Pubkey> {
        Ok(pdas::derive_node_treasury_pda(node_info)?.0)
    }

    pub fn derive_goal_vault_pda(&self, goal: &Pubkey) -> Result<Pubkey> {
        Ok(pdas::derive_goal_vault_pda(goal)?.0)
    }

    pub fn derive_contribution_pda(&self, goal: &Pubkey, contributor: &Pubkey) -> Result<Pubkey> {
        Ok(pdas::derive_contribution_pda(goal, contributor)?.0)
    }

    pub async fn get_network_config(&self) -> Result<Option<NetworkConfig>> {
        let authority = self.get_authority()?;
        let pda = pdas::derive_network_config_pda(&authority)?.0;
        self.adapter
            .get_account(&pda, NetworkConfig::from_bytes)
            .await
    }

    pub async fn get_node_info(&self, node_pubkey: &Pubkey) -> Result<Option<NodeInfo>> {
        let pda = self.derive_node_info_pda(node_pubkey)?;
        self.adapter
            .get_account(&pda, NodeInfo::from_bytes)
            .await
    }

    pub async fn get_all_nodes_by_status_and_type(&self, status: NodeStatus, node_type: NodeType) -> Result<impl Iterator<Item = NodeInfo>> {
        let filters = vec![
            AccountFilter {
                offset: 0,
                value: NODE_INFO_DISCRIMINATOR.to_vec(),
            },
            AccountFilter {
                offset: 72, // discriminator 8 + owner 32 + node_pubkey 32
                value: vec![node_type as u8],
            },
            AccountFilter {
                offset: 73, // discriminator 8 + owner 32 + node_pubkey 32 + node_type 1
                value: vec![status as u8],
            },
     
        ];
        let accounts = self.adapter
            .get_program_accounts(&DAC_ID, filters, NodeInfo::from_bytes)
            .await?.into_iter().map(|(_, node_info)| node_info);
        
        Ok(accounts)
    }

    pub async fn get_all_agents_by_status(&self, status: AgentStatus) -> Result<impl Iterator<Item = Agent>> {
        let filters = vec![
            AccountFilter {
                offset: 0,
                value: AGENT_DISCRIMINATOR.to_vec(),
            },
            AccountFilter {
                offset: 48, // discriminator 8 + agent_slot_id 8 + owner 32
                value: vec![status as u8],
            },
        ];
        let accounts = self.adapter
            .get_program_accounts(&DAC_ID, filters, Agent::from_bytes)
            .await?.into_iter().map(|(_, agent)| agent);
        
        Ok(accounts)
    }

    pub async fn get_agent(&self, network_config: &Pubkey, agent_slot_id: u64) -> Result<Option<Agent>> {
        let pda = self.derive_agent_pda(network_config, agent_slot_id)?;
        self.adapter
            .get_account(&pda, Agent::from_bytes)
            .await
    }

    pub async fn get_task(&self, network_config: &Pubkey, task_slot_id: u64) -> Result<Option<Task>> {
        let pda = self.derive_task_pda(network_config, task_slot_id)?;
        self.adapter
            .get_account(&pda, Task::from_bytes)
            .await
    }

    pub async fn get_goal(&self, network_config: &Pubkey, goal_slot_id: u64) -> Result<Option<Goal>> {
        let pda = self.derive_goal_pda(network_config, goal_slot_id)?;
        self.adapter
            .get_account(&pda, Goal::from_bytes)
            .await
    }

    pub async fn get_contribution(&self, goal: &Pubkey, contributor: &Pubkey) -> Result<Option<Contribution>> {
        let pda = self.derive_contribution_pda(goal, contributor)?;
        self.adapter
            .get_account(&pda, Contribution::from_bytes)
            .await
    }

    pub async fn get_task_by_slot(&self, task_slot_id: u64) -> Result<Task> {
        let network_config = self.derive_network_config_pda()?;
        self.get_task(&network_config, task_slot_id).await?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", task_slot_id))
    }


    async fn find_goal_by_task_slot(
        &self,
        network_config: &Pubkey,
        task_slot_id: u64,
    ) -> Result<Goal> {
        let task_pubkey = self.derive_task_pda(network_config, task_slot_id)?;
        
        let goals = self.adapter.get_program_accounts(
            &DAC_ID,
            vec![
                AccountFilter {
                    offset: 0,
                    value: GOAL_DISCRIMINATOR.to_vec(),
                },
                AccountFilter {
                    offset: 80, // discriminator 8 + goal_slot_id 8 + owner 32 + agent 32
                    value: task_pubkey.to_bytes().to_vec(),
                },
            ],
            Goal::from_bytes,
        ).await?;
        
        if goals.is_empty() {
            anyhow::bail!("Goal for task {} not found", task_slot_id);
        }
        
        if goals.len() > 1 {
            eprintln!("Warning: Multiple goals found for task {} (unexpected)", task_slot_id);
        }
        
        let (_, goal) = goals.into_iter().next().unwrap();
        Ok(goal)
    }

    pub async fn get_goal_by_task_slot(&self, task_slot_id: u64) -> Result<(Goal, Pubkey)> {
        let network_config = self.derive_network_config_pda()?;
        let goal = self.find_goal_by_task_slot(&network_config, task_slot_id).await?;
        let goal_pda = self.derive_goal_pda(&network_config, goal.goal_slot_id)?;
        Ok((goal, goal_pda))
    }

    pub async fn get_agent_by_pubkey(&self, agent_pubkey: Pubkey) -> Result<Agent> {
        self.adapter.get_account(&agent_pubkey, Agent::from_bytes).await?
            .ok_or_else(|| anyhow::anyhow!("Agent {} not found", agent_pubkey))
    }

    pub async fn register_node(
        &self,
        node_pubkey: &Pubkey,
        node_type: NodeType,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let node_info = self.derive_node_info_pda(node_pubkey)?;
        let node_treasury = self.derive_node_treasury_pda(&node_info)?;

        let instruction = RegisterNodeBuilder::new()
            .owner(self.adapter.payer_pubkey())
            .node_pubkey(*node_pubkey)
            .network_config(network_config)
            .node_info(node_info)
            .node_treasury(node_treasury)
            .system_program(solana_sdk::pubkey!("11111111111111111111111111111111"))
            .node_type(node_type)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn claim_confidential_node(
        &self,
        confidential_node_pubkey: &Pubkey,
        code_measurement: [u8; 32],
        tee_signing_pubkey: Pubkey,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let node_info = self.derive_node_info_pda(confidential_node_pubkey)?;

        let instruction = ClaimConfidentialNodeBuilder::new()
            .confidential_node(*confidential_node_pubkey)
            .network_config(network_config)
            .node_info(node_info)
            .code_measurement(code_measurement)
            .tee_signing_pubkey(tee_signing_pubkey)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn initialize_network(
        &self,
        cid_config: String,
        allocate_goals: u64,
        allocate_tasks: u64,
        approved_code_measurements: Vec<dac_client::types::CodeMeasurement>,
    ) -> Result<String> {
        let authority = self.get_authority()?;
        let network_config = pdas::derive_network_config_pda(authority)?.0;

        let instruction = InitializeNetworkBuilder::new()
            .authority(*authority)
            .network_config(network_config)
            .system_program(solana_sdk::pubkey!("11111111111111111111111111111111"))
            .cid_config(cid_config)
            .allocate_goals(allocate_goals)
            .allocate_tasks(allocate_tasks)
            .approved_code_measurements(approved_code_measurements)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn update_network_config(
        &self,
        cid_config: Option<String>,
        new_code_measurement: Option<dac_client::types::CodeMeasurement>,
    ) -> Result<String> {
        let authority = self.get_authority()?;
        let network_config = pdas::derive_network_config_pda(authority)?.0;

        let mut builder = UpdateNetworkConfigBuilder::new();
        builder
            .authority(*authority)
            .network_config(network_config);

        if let Some(cid) = cid_config {
            builder.cid_config(cid);
        }

        if let Some(measurement) = new_code_measurement {
            builder.new_code_measurement(measurement);
        }

        let instruction = builder.instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn create_agent(
        &self,
        agent_owner: &Pubkey,
        agent_config_cid: String,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let network_config_account = self.get_network_config().await?
            .ok_or_else(|| anyhow::anyhow!("Network not initialized"))?;
        let agent = self.derive_agent_pda(&network_config, network_config_account.agent_count)?;

        let instruction = CreateAgentBuilder::new()
            .agent_owner(*agent_owner)
            .network_config(network_config)
            .agent(agent)
            .system_program(solana_sdk::pubkey!("11111111111111111111111111111111"))
            .agent_config_cid(agent_config_cid)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn create_goal(
        &self,
        owner: &Pubkey,
        is_owned: bool,
        is_confidential: bool,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let network_config_account = self.get_network_config().await?
            .ok_or_else(|| anyhow::anyhow!("Network not initialized"))?;
        let goal = self.derive_goal_pda(&network_config, network_config_account.goal_count)?;

        let instruction = CreateGoalBuilder::new()
            .owner(*owner)
            .network_config(network_config)
            .goal(goal)
            .system_program(solana_sdk::pubkey!("11111111111111111111111111111111"))
            .is_owned(is_owned)
            .is_confidential(is_confidential)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }


    pub async fn claim_public_node(
        &self,
        public_node_pubkey: &Pubkey,
        node_info_cid: String,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let node_info = self.derive_node_info_pda(public_node_pubkey)?;

        let instruction = ClaimPublicNodeBuilder::new()
            .node(*public_node_pubkey)
            .network_config(network_config)
            .node_info(node_info)
            .node_info_cid(node_info_cid)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn claim_task(
        &self,
        task_slot_id: u64,
        max_task_cost: u64,
    ) -> Result<String> {
        let node_pubkey = self.adapter.payer_pubkey();
        let network_config = self.derive_network_config_pda()?;
        let goal = self.find_goal_by_task_slot(&network_config, task_slot_id).await?;
        let goal_slot_id = goal.goal_slot_id;

        let node_info = self.derive_node_info_pda(&node_pubkey)?;
        let goal = self.derive_goal_pda(&network_config, goal_slot_id)?;
        let vault = self.derive_goal_vault_pda(&goal)?;
        let task = self.derive_task_pda(&network_config, task_slot_id)?;

        let instruction = ClaimTaskBuilder::new()
            .compute_node(node_pubkey)
            .task(task)
            .goal(goal)
            .vault(vault)
            .compute_node_info(node_info)
            .network_config(network_config)
            .max_task_cost(max_task_cost)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }


    pub async fn submit_task_result(
        &self,
        task_slot_id: u64,
        input_cid: String,
        output_cid: String,
        next_input_cid: String,
    ) -> Result<String> {
        let node_pubkey = self.adapter.payer_pubkey();
        let network_config = self.derive_network_config_pda()?;
        let task = self.derive_task_pda(&network_config, task_slot_id)?;

        let instruction = SubmitTaskResultBuilder::new()
            .compute_node(node_pubkey)
            .network_config(network_config)
            .task(task)
            .input_cid(input_cid)
            .output_cid(output_cid)
            .next_input_cid(next_input_cid)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn submit_confidential_task_validation(
        &self,
        validator_pubkey: &Pubkey,
        goal_slot_id: u64,
        task_slot_id: u64,
        ed25519_instruction: solana_sdk::instruction::Instruction,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let validator_node_info = self.derive_node_info_pda(validator_pubkey)?;
        let goal = self.derive_goal_pda(&network_config, goal_slot_id)?;
        let vault = self.derive_goal_vault_pda(&goal)?;
        let task = self.derive_task_pda(&network_config, task_slot_id)?;
        
        let task_account = self.get_task(&network_config, task_slot_id).await?
            .ok_or_else(|| anyhow::anyhow!("Task not found"))?;
        let node_pubkey = task_account.compute_node
            .ok_or_else(|| anyhow::anyhow!("Task has no assigned node"))?;
        let node_info = self.derive_node_info_pda(&node_pubkey)?;
        let node_treasury = self.derive_node_treasury_pda(&node_info)?;

        let instruction = SubmitConfidentialTaskValidationBuilder::new()
            .node_validating(*validator_pubkey)
            .goal(goal)
            .vault(vault)
            .task(task)
            .node_info(node_info)
            .node_treasury(node_treasury)
            .validator_node_info(validator_node_info)
            .network_config(network_config)
            .instruction_sysvar(solana_sdk::pubkey!("Sysvar1nstructions1111111111111111111111111"))
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[ed25519_instruction, instruction])
            .await
    }

    pub async fn submit_public_task_validation(
        &self,
        validator_pubkey: &Pubkey,
        goal_slot_id: u64,
        task_slot_id: u64,
        payment_amount: u64,
        approved: bool,
        goal_completed: bool,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let validator_node_info = self.derive_node_info_pda(validator_pubkey)?;
        let goal = self.derive_goal_pda(&network_config, goal_slot_id)?;
        let vault = self.derive_goal_vault_pda(&goal)?;
        let task = self.derive_task_pda(&network_config, task_slot_id)?;
        
        let task_account = self.get_task(&network_config, task_slot_id).await?
            .ok_or_else(|| anyhow::anyhow!("Task not found"))?;
        let node_pubkey = task_account.compute_node
            .ok_or_else(|| anyhow::anyhow!("Task has no assigned node"))?;
        let node_info = self.derive_node_info_pda(&node_pubkey)?;
        let node_treasury = self.derive_node_treasury_pda(&node_info)?;

        let instruction = SubmitPublicTaskValidationBuilder::new()
            .node_validating(*validator_pubkey)
            .goal(goal)
            .vault(vault)
            .task(task)
            .node_info(node_info)
            .node_treasury(node_treasury)
            .validator_node_info(validator_node_info)
            .network_config(network_config)
            .instruction_sysvar(solana_sdk::pubkey!("Sysvar1nstructions1111111111111111111111111"))
            .payment_amount(payment_amount)
            .approved(approved)
            .goal_completed(goal_completed)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn validate_agent(
        &self,
        validator_pubkey: &Pubkey,
        agent_slot_id: u64,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let agent = self.derive_agent_pda(&network_config, agent_slot_id)?;
        let node_info = self.derive_node_info_pda(validator_pubkey)?;

        let instruction = ValidateAgentBuilder::new()
            .node(*validator_pubkey)
            .agent(agent)
            .node_info(node_info)
            .network_config(network_config)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn validate_public_node(
        &self,
        validator_pubkey: &Pubkey,
        public_node_pubkey: &Pubkey,
        approved: bool,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let validator_node_info = self.derive_node_info_pda(validator_pubkey)?;
        let public_node_info = self.derive_node_info_pda(public_node_pubkey)?;

        let validate_instruction = ValidatePublicNodeBuilder::new()
            .node_validating(*validator_pubkey)
            .network_config(network_config)
            .node_validating_info(validator_node_info)
            .node_info(public_node_info)
            .approved(approved)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[validate_instruction])
            .await
    }

    pub fn subscribe_to_node_info(
        &self,
        node_pubkey: Option<&Pubkey>,
        node_type: Option<NodeType>,
        status: Option<NodeStatus>,
        tx: mpsc::Sender<NodeInfo>,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let mut filters = vec![
            AccountFilter {
                offset: 0,
                value: NODE_INFO_DISCRIMINATOR.to_vec(),
            },
        ];

        if let Some(node_pubkey) = node_pubkey {
            filters.push(AccountFilter {
                offset: 40, // discriminator 8 + owner 32
                value: node_pubkey.to_bytes().to_vec(),
            });
        }

        if let Some(node_type) = node_type {
            filters.push(AccountFilter {
                offset: 72, // discriminator 8 + owner 32 + node_pubkey 32
                value: vec![node_type as u8],
            });
        }

        if let Some(status) = status {
            let status_byte = status as u8;
            filters.push(AccountFilter {
                offset: 73, // discriminator 8 + owner 32 + node_pubkey 32 + node_type 1
                value: vec![status_byte],
            });
        }

        Ok(self.adapter.watch_program_accounts(
            &DAC_ID,
            filters,
            NodeInfo::from_bytes,
            tx,
            "node info",
        ))
    }

    pub fn subscribe_to_agents(
        &self,
        status: Option<AgentStatus>,
        tx: mpsc::Sender<Agent>,
    ) -> tokio::task::JoinHandle<()> {
        let mut filters = vec![AccountFilter {
            offset: 0,
            value: AGENT_DISCRIMINATOR.to_vec(),
        }];

        if let Some(status) = status {
            let status_byte = status as u8;
            filters.push(AccountFilter {
                offset: 48, // discriminator 8 + agent_slot_id 8 + owner 32
                value: vec![status_byte],
            });
        }

        self.adapter.watch_program_accounts(
            &DAC_ID,
            filters,
            Agent::from_bytes,
            tx,
            "agents",
        )
    }

    pub fn subscribe_to_tasks(
        &self,
        status: Option<TaskStatus>,
        tx: mpsc::Sender<Task>,
    ) -> tokio::task::JoinHandle<()> {
        let mut filters = vec![AccountFilter {
            offset: 0,
            value: TASK_DISCRIMINATOR.to_vec(),
        }];

        if let Some(status) = status {
            let status_byte = status as u8;
            filters.push(AccountFilter {
                offset: 49, // discriminator 8 + task_slot_id 8 + action_type 1 + agent 32
                value: vec![status_byte],
            });
        }

        self.adapter.watch_program_accounts(
            &DAC_ID,
            filters,
            Task::from_bytes,
            tx,
            "tasks",
        )
    }

    pub fn subscribe_to_goals(
        &self,
        status: Option<GoalStatus>,
        tx: mpsc::Sender<Goal>,
    ) -> tokio::task::JoinHandle<()> {
        let mut filters = vec![AccountFilter {
            offset: 0,
            value: GOAL_DISCRIMINATOR.to_vec(),
        }];

        if let Some(status) = status {
            let status_byte = status as u8;
            filters.push(AccountFilter {
                offset: 112, // discriminator 8 + goal_slot_id 8 + owner 32 + agent 32 + task 32
                value: vec![status_byte],
            });
        }

        self.adapter.watch_program_accounts(
            &DAC_ID,
            filters,
            Goal::from_bytes,
            tx,
            "goals",
        )
    }
}
