use anyhow::Result;
use dac_sdk::{DacClient, types::AgentStatus};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct AgentService {
    dac_client: Arc<DacClient>,
    node_pubkey: Pubkey,
}

impl AgentService {
    pub fn new(
        dac_client: Arc<DacClient>,
        node_pubkey: Pubkey,
    ) -> Self {
        Self {
            dac_client,
            node_pubkey,
        }
    }

    pub async fn validate_agents_with_pending_status(&self) -> Result<()> {
        let agents = self.dac_client.get_all_agents_by_status(AgentStatus::Pending).await?;
        for agent in agents {
            println!("Agent awaiting validation: slot_id={}", agent.agent_slot_id);
            if let Err(e) = self.validate_agent(agent.agent_slot_id).await {
                eprintln!("Failed to validate agent: {}", e);
            }
        }
        Ok(())
    }

    pub async fn run_agent_validation_loop(&self, shutdown: CancellationToken) -> Result<()> {
        println!("Validating existing agents with pending status...");
        self.validate_agents_with_pending_status().await?;
        
        println!("Starting monitoring for new agents with pending status...");
        let (tx, mut rx) = mpsc::channel(1);
        let _subscription = self.dac_client.subscribe_to_agents(
            Some(AgentStatus::Pending),
            tx,
            shutdown.clone(),
        );

        println!("Monitoring for agents to validate...");
        
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(agent) => {
                            println!("Agent awaiting validation: slot_id={}", agent.agent_slot_id);
                            match self.validate_agent(agent.agent_slot_id).await {
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
                    return Ok(());
                }
            }
        }
    }

    pub async fn validate_agent(&self, agent_slot_id: u64) -> Result<String> {
        self.dac_client
            .validate_agent(&self.node_pubkey, agent_slot_id)
            .await
    }
}
