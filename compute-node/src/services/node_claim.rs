use anyhow::Result;
use dac_sdk::{SolanaAdapter, DacClient, types::NodeStatus};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct NodeClaimService {
    dac_client: DacClient,
}

impl NodeClaimService {
    pub fn new(adapter: Arc<SolanaAdapter>) -> Self {
        Self {
            dac_client: DacClient::new(adapter),
        }
    }

    pub async fn handle_node_claim(&self, node_pubkey: &Pubkey) -> Result<bool> {
        if self.try_claim_node(node_pubkey).await? {
            println!("Node claimed successfully.");
            return Ok(true);
        }
        
        println!("Node not registered. Monitoring for node registration...");
        let claimed = self.monitor_node_account(node_pubkey).await?;
        
        if claimed {
            println!("Node claimed successfully after monitoring.");
            Ok(true)
        } else {
            println!("Node monitoring interrupted. Node not claimed.");
            Ok(false)
        }
    }

    pub async fn try_claim_node(&self, node_pubkey: &Pubkey) -> Result<bool> {
        match self.dac_client.get_node_info(node_pubkey).await? {
            Some(node_info) => {
                match node_info.status {
                    NodeStatus::PendingClaim => {
                        println!("Node registered but not claimed. Registering as compute node...");
                        // TODO: Upload node metadata to IPFS and use the CID here
                        let tx = self.dac_client.claim_compute_node(node_pubkey, String::new()).await?;
                        println!("Transaction: {:?}", tx);
                        Ok(true)
                    }
                    NodeStatus::Active => {
                        println!("Node already active. Status: {:?}", node_info.status);
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

    pub async fn monitor_node_account(&self, node_pubkey: &Pubkey) -> Result<bool> {
        let (tx, mut rx) = mpsc::channel(1);
        let handle = self.dac_client.subscribe_to_node_info(node_pubkey, Some(NodeStatus::PendingClaim), tx)?;

        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(compute_node_info) => {
                            println!("Compute node info received: status={:?}", compute_node_info.status);

                            if compute_node_info.status == NodeStatus::PendingClaim {
                                let node_info_cid = compute_node_info.node_info_cid
                                    .unwrap_or_else(|| String::new());
                                
                                match self.dac_client.claim_compute_node(
                                    node_pubkey,
                                    node_info_cid,
                                ).await {
                                    Ok(sig) => {
                                        println!("Successfully claimed compute node. Transaction: {:?}", sig);
                                        handle.abort();
                                        return Ok(true);
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to claim node: {}", e);
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
