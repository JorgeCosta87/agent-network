use anyhow::Result;
use dac_sdk::{SolanaAdapter, DacClient, types::{NodeStatus, NodeType}};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

pub struct NodeRegistryService {
    dac_client: DacClient,
}

impl NodeRegistryService {
    pub fn new(adapter: Arc<SolanaAdapter>) -> Self {
        Self {
            dac_client: DacClient::new(adapter),
        }
    }

    pub async fn check_and_claim_pending_node(&self, node_pubkey: &Pubkey) -> Result<()> {
        match self.dac_client.get_node_info(node_pubkey).await? {
            Some(node_info) => {
                match node_info.status {
                    NodeStatus::PendingClaim => {
                        println!("Node registered but not claimed. Registering as compute node...");
                        self.register_compute_node(node_pubkey).await?;
                    }
                    NodeStatus::Active => {
                        println!("Node already active. Status: {:?}", node_info.status);
                    }
                    _ => {
                        println!("Node status: {:?}", node_info.status);
                    }
                }
            }
            None => {
                println!("Node not registered. Registering compute node...");
                self.register_compute_node(node_pubkey).await?;
            }
        }

        Ok(())
    }

    async fn register_compute_node(&self, node_pubkey: &Pubkey) -> Result<()> {
        let signature = self
            .dac_client
            .register_node(node_pubkey, NodeType::Compute)
            .await?;

        println!("Registered compute node. Signature: {}", signature);
        Ok(())
    }
}
