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
        ClaimValidatorNodeBuilder, RegisterNodeBuilder, 
        SubmitTaskResultBuilder, ClaimComputeNodeBuilder,
        ClaimTaskBuilder, CreateAgentBuilder, CreateGoalBuilder,
        SetGoalBuilder, ContributeToGoalBuilder, WithdrawFromGoalBuilder,
        SubmitTaskValidationBuilder, ValidateAgentBuilder,
        ValidateComputeNodeBuilder, InitializeNetworkBuilder,
        UpdateNetworkConfigBuilder,
    },
    types::{NodeType, TaskStatus, AgentStatus, GoalStatus, NodeStatus},
    programs::DAC_ID,
};

use crate::pdas;

pub struct DacClient {
    adapter: Arc<SolanaAdapter>,
}

impl DacClient {
    pub fn new(adapter: Arc<SolanaAdapter>) -> Self {
        Self { adapter }
    }

    pub fn derive_network_config_pda(&self) -> Result<Pubkey> {
        Ok(pdas::derive_network_config_pda()?.0)
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
        let pda = self.derive_network_config_pda()?;
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

    pub async fn claim_validator_node(
        &self,
        validator_node_pubkey: &Pubkey,
        code_measurement: [u8; 32],
        tee_signing_pubkey: Pubkey,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let node_info = self.derive_node_info_pda(validator_node_pubkey)?;

        let instruction = ClaimValidatorNodeBuilder::new()
            .validator_node(*validator_node_pubkey)
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
        authority: &Pubkey,
        cid_config: String,
        allocate_goals: u64,
        allocate_tasks: u64,
        approved_code_measurements: Vec<dac_client::types::CodeMeasurement>,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;

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
        authority: &Pubkey,
        cid_config: Option<String>,
        new_code_measurement: Option<dac_client::types::CodeMeasurement>,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;

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
        is_public: bool,
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
            .is_public(is_public)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn set_goal(
        &self,
        owner: &Pubkey,
        goal_slot_id: u64,
        agent_slot_id: u64,
        task_slot_id: u64,
        specification_cid: String,
        max_iterations: u64,
        initial_deposit: u64,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let goal = self.derive_goal_pda(&network_config, goal_slot_id)?;
        let vault = self.derive_goal_vault_pda(&goal)?;
        let owner_contribution = self.derive_contribution_pda(&goal, owner)?;
        let task = self.derive_task_pda(&network_config, task_slot_id)?;
        let agent = self.derive_agent_pda(&network_config, agent_slot_id)?;

        let instruction = SetGoalBuilder::new()
            .owner(*owner)
            .goal(goal)
            .vault(vault)
            .owner_contribution(owner_contribution)
            .task(task)
            .agent(agent)
            .network_config(network_config)
            .system_program(solana_sdk::pubkey!("11111111111111111111111111111111"))
            .specification_cid(specification_cid)
            .max_iterations(max_iterations)
            .initial_deposit(initial_deposit)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn claim_compute_node(
        &self,
        compute_node_pubkey: &Pubkey,
        node_info_cid: String,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let node_info = self.derive_node_info_pda(compute_node_pubkey)?;

        let instruction = ClaimComputeNodeBuilder::new()
            .compute_node(*compute_node_pubkey)
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
        compute_node_pubkey: &Pubkey,
        goal_slot_id: u64,
        task_slot_id: u64,
        max_task_cost: u64,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let compute_node_info = self.derive_node_info_pda(compute_node_pubkey)?;
        let goal = self.derive_goal_pda(&network_config, goal_slot_id)?;
        let vault = self.derive_goal_vault_pda(&goal)?;
        let task = self.derive_task_pda(&network_config, task_slot_id)?;

        let instruction = ClaimTaskBuilder::new()
            .compute_node(*compute_node_pubkey)
            .task(task)
            .goal(goal)
            .vault(vault)
            .compute_node_info(compute_node_info)
            .network_config(network_config)
            .max_task_cost(max_task_cost)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn contribute_to_goal(
        &self,
        contributor: &Pubkey,
        goal_slot_id: u64,
        deposit_amount: u64,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let goal = self.derive_goal_pda(&network_config, goal_slot_id)?;
        let vault = self.derive_goal_vault_pda(&goal)?;
        let contribution = self.derive_contribution_pda(&goal, contributor)?;

        let instruction = ContributeToGoalBuilder::new()
            .contributor(*contributor)
            .goal(goal)
            .vault(vault)
            .contribution(contribution)
            .network_config(network_config)
            .system_program(solana_sdk::pubkey!("11111111111111111111111111111111"))
            .deposit_amount(deposit_amount)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn withdraw_from_goal(
        &self,
        contributor: &Pubkey,
        goal_slot_id: u64,
        shares_to_burn: u64,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let goal = self.derive_goal_pda(&network_config, goal_slot_id)?;
        let vault = self.derive_goal_vault_pda(&goal)?;
        let contribution = self.derive_contribution_pda(&goal, contributor)?;

        let instruction = WithdrawFromGoalBuilder::new()
            .contributor(*contributor)
            .goal(goal)
            .vault(vault)
            .contribution(contribution)
            .network_config(network_config)
            .system_program(solana_sdk::pubkey!("11111111111111111111111111111111"))
            .shares_to_burn(shares_to_burn)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn submit_task_result(
        &self,
        compute_node_pubkey: &Pubkey,
        task_slot_id: u64,
        input_cid: String,
        output_cid: String,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let task = self.derive_task_pda(&network_config, task_slot_id)?;

        let instruction = SubmitTaskResultBuilder::new()
            .compute_node(*compute_node_pubkey)
            .network_config(network_config)
            .task(task)
            .input_cid(input_cid)
            .output_cid(output_cid)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn submit_task_validation(
        &self,
        validator_pubkey: &Pubkey,
        goal_slot_id: u64,
        task_slot_id: u64,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let validator_node_info = self.derive_node_info_pda(validator_pubkey)?;
        let goal = self.derive_goal_pda(&network_config, goal_slot_id)?;
        let vault = self.derive_goal_vault_pda(&goal)?;
        let task = self.derive_task_pda(&network_config, task_slot_id)?;
        
        let task_account = self.get_task(&network_config, task_slot_id).await?
            .ok_or_else(|| anyhow::anyhow!("Task not found"))?;
        let compute_node_pubkey = task_account.compute_node
            .ok_or_else(|| anyhow::anyhow!("Task has no assigned compute node"))?;
        let compute_node_info = self.derive_node_info_pda(&compute_node_pubkey)?;
        let node_treasury = self.derive_node_treasury_pda(&compute_node_info)?;

        let instruction = SubmitTaskValidationBuilder::new()
            .validator(*validator_pubkey)
            .goal(goal)
            .vault(vault)
            .task(task)
            .compute_node_info(compute_node_info)
            .node_treasury(node_treasury)
            .validator_node_info(validator_node_info)
            .network_config(network_config)
            .instruction_sysvar(solana_sdk::pubkey!("Sysvar1nstructions1111111111111111111111111"))
            .system_program(solana_sdk::pubkey!("11111111111111111111111111111111"))
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

        let instruction = ValidateAgentBuilder::new()
            .validator(*validator_pubkey)
            .agent(agent)
            .network_config(network_config)
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub async fn validate_compute_node(
        &self,
        validator_pubkey: &Pubkey,
        compute_node_pubkey: &Pubkey,
    ) -> Result<String> {
        let network_config = self.derive_network_config_pda()?;
        let validator_node_info = self.derive_node_info_pda(validator_pubkey)?;
        let compute_node_info = self.derive_node_info_pda(compute_node_pubkey)?;

        let instruction = ValidateComputeNodeBuilder::new()
            .validator_node_pubkey(*validator_pubkey)
            .validator_node_info(validator_node_info)
            .compute_node_info(compute_node_info)
            .network_config(network_config)
            .instruction_sysvar(solana_sdk::pubkey!("Sysvar1nstructions1111111111111111111111111"))
            .instruction();

        self.adapter
            .send_and_confirm_transaction(&[instruction])
            .await
    }

    pub fn subscribe_to_node_info(
        &self,
        node_pubkey: &Pubkey,
        status: Option<NodeStatus>,
        tx: mpsc::Sender<NodeInfo>,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let mut filters = vec![
            AccountFilter {
                offset: 0,
                value: NODE_INFO_DISCRIMINATOR.to_vec(),
            },
            AccountFilter {
                offset: 40, // discriminator 8 + owner 32
                value: node_pubkey.to_bytes().to_vec(),
            },
        ];

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
