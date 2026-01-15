use anyhow::Result;
use dac_sdk::{DacClient, SolanaAdapter, accounts::NodeInfo, types::{NodeStatus, NodeType}};
use ipfs_adapter::IpfsClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use crate::utils::{SystemInfoCollector, ModelValidator, ModelConfig};
use serde_yaml;

pub struct NodeClaimService {
    dac_client: DacClient,
    ipfs_client: Arc<IpfsClient>,
}

impl NodeClaimService {
    pub fn new(adapter: Arc<SolanaAdapter>, authority: Pubkey, ipfs_client: Arc<IpfsClient>) -> Self {
        let dac_client = DacClient::new(adapter, authority);
        Self { dac_client, ipfs_client }
    }

    /// Collect system information, validate models, and upload to IPFS
    pub async fn collect_and_upload_node_config(
        &self,
        node_pubkey: &Pubkey,
        model_configs: Vec<ModelConfig>,
    ) -> Result<String> {
        println!("Collecting system information...");
        
        // Collect hardware info first to get GPU VRAM
        let hardware = SystemInfoCollector::collect_hardware_info().await?;
        let gpu_vram: Vec<u32> = hardware.gpu.iter().map(|g| g.vram_gb).collect();
        
        println!("System information collected:");
        println!("  CPU: {} ({} cores)", hardware.cpu.model, hardware.cpu.cores);
        println!("  GPUs: {}", hardware.gpu.len());
        for (i, gpu) in hardware.gpu.iter().enumerate() {
            println!("    GPU {}: {} ({} GB VRAM)", i, gpu.name, gpu.vram_gb);
        }
        
        // Get network config CID from on-chain
        let network_config_account = self.dac_client.get_network_config().await?
            .ok_or_else(|| anyhow::anyhow!("Network config not found on-chain"))?;
        
        // Validate models against network config
        println!("Validating models against network config...");
        let validated_models = if !model_configs.is_empty() {
            ModelValidator::validate_available_models(
                &network_config_account.cid_config,
                model_configs,
                gpu_vram,
                &self.ipfs_client,
            ).await?
        } else {
            Vec::new()
        };
        
        println!("Validated models:");
        for model in &validated_models {
            println!("  {} ({} quantization, {} GB VRAM, GPU {}, {} concurrent requests)", 
                model.model_name, model.quantization, model.vram_required_gb, model.gpu_index, model.max_concurrent_requests);
        }
        
        // Collect full node config with validated models
        let node_config = SystemInfoCollector::collect_node_config(
            node_pubkey.to_string(),
            "DAC Compute Node".to_string(),
            "High-performance compute node for LLM inference".to_string(),
            None,
            validated_models,
        ).await?;

        println!("  CUDA: {}", node_config.software.cuda_version);

        // Convert to YAML
        let yaml_content = serde_yaml::to_string(&node_config)
            .map_err(|e| anyhow::anyhow!("Failed to serialize node config to YAML: {}", e))?;

        // Upload to IPFS
        println!("Uploading node configuration to IPFS...");
        let cid = self.ipfs_client
            .add(yaml_content.into_bytes())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to upload node config to IPFS: {}", e))?;

        println!("✅ Node configuration uploaded to IPFS: {}", cid);
        Ok(cid)
    }

    pub async fn handle_node_claim_and_validation(
        &self,
        node_pubkey: &Pubkey,
        model_configs: Vec<ModelConfig>,
    ) -> Result<bool> {
        // Step 1: Try to claim the node
        let node_info = self.try_claim_node(node_pubkey, model_configs.clone()).await?;
        
        if node_info.is_none() {
            println!("Node not registered. Monitoring for node registration...");
            let claimed = self.monitor_node_account(node_pubkey, model_configs.clone()).await?;
            if !claimed {
                println!("Node monitoring interrupted. Node not claimed.");
                return Ok(false);
            }
        }
        
        // Step 2: Check current status after claiming
        let node_info = self.dac_client.get_node_info(node_pubkey).await?
            .ok_or_else(|| anyhow::anyhow!("Node info not found after claiming"))?;
        
        println!("Node status after claim: {:?}", node_info.status);
        
        // Step 3: If already active, we're done
        if node_info.status == NodeStatus::Active {
            println!("✅ Node is already active. Ready to start services.");
            return Ok(true);
        }
        
        // Step 4: If awaiting validation, wait for activation
        if node_info.status == NodeStatus::AwaitingValidation {
            println!("⏳ Node is awaiting validation. Waiting for authority to activate...");
            return self.wait_for_activation(node_pubkey).await;
        }
        
        // Step 5: Handle other statuses
        println!("Node status: {:?}. Expected PendingClaim, AwaitingValidation, or Active.", node_info.status);
        Ok(false)
    }

    pub async fn try_claim_node(&self, node_pubkey: &Pubkey, model_configs: Vec<ModelConfig>) -> Result<Option<NodeInfo>> {
        match self.dac_client.get_node_info(node_pubkey).await? {
            Some(node_info) => {
                match node_info.status {
                    NodeStatus::PendingClaim => {
                        println!("Node registered but not claimed. Claiming as public node...");
                        
                        // Collect system info and upload to IPFS
                        let node_info_cid = self.collect_and_upload_node_config(node_pubkey, model_configs).await?;
                        
                        let tx = self.dac_client.claim_public_node(node_pubkey, node_info_cid).await?;
                        println!("Transaction: {:?}", tx);
                        Ok(Some(node_info))
                    }
                    _ => {
                        Ok(Some(node_info))
                    }
                }
            }
            None => {
                Ok(None)
            }
        }
    }

    pub async fn monitor_node_account(&self, node_pubkey: &Pubkey, model_configs: Vec<ModelConfig>) -> Result<bool> {
        let (tx, mut rx) = mpsc::channel(1);
        let cancel_token = CancellationToken::new();
        
        let handle = self.dac_client.subscribe_to_node_info(
            Some(node_pubkey),
            Some(NodeType::Public),
            Some(NodeStatus::PendingClaim),
            tx,
            cancel_token.clone(),
        )?;
        
        let mut success = false;
        
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(node_info) => {
                            println!("Public node info received: status={:?}", node_info.status);

                            if node_info.status == NodeStatus::PendingClaim {
                                // Collect system info and upload to IPFS if not already set
                                let node_info_cid = if node_info.node_info_cid.is_some() {
                                    node_info.node_info_cid.unwrap()
                                } else {
                                    self.collect_and_upload_node_config(node_pubkey, model_configs.clone()).await?
                                };
                                
                                match self.dac_client.claim_public_node(
                                    node_pubkey,
                                    node_info_cid,
                                ).await {
                                    Ok(sig) => {
                                        println!("Successfully claimed public node. Transaction: {:?}", sig);
                                        success = true;
                                        break;
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
                            break;
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("Node monitoring interrupted by user");
                    break;
                }
            }
        }
        
        cancel_token.cancel();
        let _ = handle.await;
        
        Ok(success)
    }


    pub async fn wait_for_activation(&self, node_pubkey: &Pubkey) -> Result<bool> {
        let (tx, mut rx) = mpsc::channel(1);
        let cancel_token = CancellationToken::new();
        
        let handle = self.dac_client.subscribe_to_node_info(
            Some(node_pubkey),
            Some(NodeType::Public),
            None,
            tx,
            cancel_token.clone(),
        )?;

        println!("Subscribed to node status updates. Waiting for activation...");

        let mut success = false;

        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Some(node_info) => {
                            println!("Node status update: {:?}", node_info.status);
                            
                            match node_info.status {
                                NodeStatus::Active => {
                                    println!("Node activated successfully! Ready to start services.");
                                    success = true;
                                    break;
                                }
                                NodeStatus::AwaitingValidation => {
                                    println!("Still awaiting validation...");
                                }
                                NodeStatus::Rejected => {
                                    println!("Node was rejected. Cannot proceed.");
                                    break;
                                }
                                NodeStatus::Disabled => {
                                    println!("Node was disabled. Cannot proceed.");
                                    break;
                                }
                                _ => {
                                    println!("ℹNode status: {:?}", node_info.status);
                                }
                            }
                        }
                        None => {
                            println!("Node status subscription channel closed");
                            break;
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("Node status monitoring interrupted by user");
                    break;
                }
            }
        }

        cancel_token.cancel();
        let _ = handle.await;
        Ok(success)
    }
}
