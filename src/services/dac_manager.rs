use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;
use crate::adapters::{SolanaAdapter, AccountFilter};
use solana_sdk::pubkey::Pubkey;
use sol_mind_protocol_client::dac_manager::{
    accounts::{
        AGENT_DISCRIMINATOR, Agent, COMPUTE_NODE_INFO_DISCRIMINATOR, ComputeNodeInfo,
    },
    instructions::{ActivateAgentBuilder, ClaimComputeNodeBuilder},
    programs::DAC_MANAGER_ID,
};

pub struct DacManagerService {
    adapter: Arc<SolanaAdapter>,
}

impl DacManagerService {
    pub fn new(adapter: Arc<SolanaAdapter>) -> Self {
        Self { adapter }
    }

    pub fn derive_compute_node_info_pda(&self, node_pubkey: &Pubkey) -> Result<Pubkey> {
        let (address, _) = Pubkey::try_find_program_address(
            &[b"compute_node", &node_pubkey.as_ref()],
            &DAC_MANAGER_ID,
        )
        .ok_or_else(|| anyhow::anyhow!("Failed to derive compute node address"))?;
        Ok(address)
    }

    pub fn derive_agent_pda(&self, owner: &Pubkey, agent_id: u64) -> Result<Pubkey> {
        let (address, _) = Pubkey::try_find_program_address(
            &[
                b"agent",
                owner.as_ref(),
                &agent_id.to_le_bytes(),
            ],
            &DAC_MANAGER_ID,
        )
        .ok_or_else(|| anyhow::anyhow!("Failed to derive agent address"))?;
        Ok(address)
    }

    pub async fn get_agents_accounts(
        &self,
        node_pubkey: &Pubkey,
    ) -> Result<Option<Vec<(Pubkey, Agent)>>> {
        let accounts = self
            .adapter
            .get_program_accounts(
                &DAC_MANAGER_ID,
                vec![
                    AccountFilter {
                        offset: 0,
                        value: AGENT_DISCRIMINATOR.to_vec(),
                    },
                    AccountFilter {
                        offset: 8 + 8 + 32,
                        value: node_pubkey.as_ref().to_vec(),
                    },
                ],
                Agent::from_bytes,
            )
            .await?;

        Ok(Some(accounts))
    }

    pub async fn get_compute_node_info_account(
        &self,
        node_pubkey: &Pubkey,
    ) -> Result<Option<ComputeNodeInfo>> {
        let pda = self.derive_compute_node_info_pda(node_pubkey)?;
        self.get_compute_node_info_by_address(pda).await
    }

    pub async fn get_compute_node_info_by_address(
        &self,
        address: Pubkey,
    ) -> Result<Option<ComputeNodeInfo>> {
        self.adapter
            .get_account(&address, ComputeNodeInfo::from_bytes)
            .await
    }

    pub async fn claim_node(
        &self,
        compute_node_address: &Pubkey,
        node_info_cid: String,
    ) -> Result<String> {
        let compute_node_info_pda = self.derive_compute_node_info_pda(compute_node_address)?;

        let instruction = ClaimComputeNodeBuilder::new()
            .payer(self.adapter.payer_pubkey())
            .compute_node(compute_node_address.clone())
            .compute_node_info(compute_node_info_pda)
            .node_info_cid(node_info_cid)
            .instruction();

        let signature = self
            .adapter
            .send_and_confirm_transaction(&[instruction])
            .await?;

        Ok(signature)
    }

    pub async fn activate_agent(&self, agent: &Agent) -> Result<String> {
        let agent_pda = self.derive_agent_pda(&agent.owner, agent.agent_id)?;

        let instruction = ActivateAgentBuilder::new()
            .payer(self.adapter.payer_pubkey())
            .compute_node(self.adapter.payer_pubkey())
            .agent(agent_pda)
            .agent_id(agent.agent_id)
            .instruction();

        let signature = self
            .adapter
            .send_and_confirm_transaction(&[instruction])
            .await?;

        Ok(signature)
    }

    pub fn watch_compute_node_info_accounts(
        &self,
        node_pubkey: &Pubkey,
        tx: mpsc::Sender<ComputeNodeInfo>,
    ) -> tokio::task::JoinHandle<()> {
        self.adapter.watch_program_accounts(
            &DAC_MANAGER_ID,
            vec![
                AccountFilter {
                    offset: 0,
                    value: COMPUTE_NODE_INFO_DISCRIMINATOR.to_vec(),
                },
                AccountFilter {
                    offset: 8 + 32,
                    value: node_pubkey.as_ref().to_vec(),
                },
            ],
            ComputeNodeInfo::from_bytes,
            tx,
            "compute node info accounts",
        )
    }

    pub fn watch_agent_accounts(
        &self,
        compute_node_pubkey: &Pubkey,
        tx: mpsc::Sender<Agent>,
    ) -> tokio::task::JoinHandle<()> {
        self.adapter.watch_program_accounts(
            &DAC_MANAGER_ID,
            vec![
                AccountFilter {
                    offset: 0,
                    value: AGENT_DISCRIMINATOR.to_vec(),
                },
                AccountFilter {
                    offset: 8 + 8 + 32,
                    value: compute_node_pubkey.as_ref().to_vec(),
                },
            ],
            Agent::from_bytes,
            tx,
            "agent accounts",
        )
    }
}
