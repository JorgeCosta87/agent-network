mod config;
mod services;
mod utils;

use std::sync::Arc;
use anyhow::{Result, Context};
use config::Config;
use dac_sdk::{SolanaAdapter, DacClient};
use ipfs_adapter::IpfsClient;
use services::{NodeClaimService, TaskExecutionService, TaskValidationService, AgentService};
use utils::NodeSettings;
use tokio_util::sync::CancellationToken;


#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let keypair_path = args.get(1).map(|s| s.as_str());
    
    let config = Config::load(keypair_path)?;

    println!("Starting DAC Compute Node...");
    println!("Node pubkey: {}", config.node_pubkey());
    println!("RPC URL: {}", config.rpc_url);
    println!("RPC WebSocket URL: {}", config.rpc_websocket_url);
    println!("IPFS URL: {}", config.ipfs_url);

    let solana_adapter = Arc::new(SolanaAdapter::new(
        config.rpc_url.clone(),
        config.rpc_websocket_url.clone(),
        config.node_keypair.insecure_clone()
    ).await);

    startup_checks(&config, &solana_adapter).await?;

    let dac_client = Arc::new(DacClient::new(Arc::clone(&solana_adapter), config.network_authority));
    let ipfs_adapter = Arc::new(IpfsClient::new(&config.ipfs_url));
    println!("IPFS client initialized: {}", config.ipfs_url);

    let node_claim_service = Arc::new(
        NodeClaimService::new(Arc::clone(&solana_adapter), config.network_authority, Arc::clone(&ipfs_adapter))
    );

    let node_pubkey = config.node_pubkey();
    
    let node_settings_path = std::env::var("NODE_SETTINGS_PATH")
        .context("NODE_SETTINGS_PATH environment variable not set")?;
    let node_settings = NodeSettings::load_from_file(&node_settings_path)?;
    println!("Loaded {} model config(s) from {}", node_settings.models.len(), node_settings_path);
    
    // Claim node and wait for validation
    let is_node_active = node_claim_service.handle_node_claim_and_validation(
        &node_pubkey,
        node_settings.models,
    ).await?;
    
    if !is_node_active {
        println!("Node is not active. Exiting.");
        return Ok(());
    }
    
    println!("🚀 Node is active. Starting task execution and validation monitoring...");


    let task_execution_service = Arc::new(
        TaskExecutionService::new(
            Arc::clone(&dac_client),
            Arc::clone(&ipfs_adapter),
        )
    );
    let task_validation_service = Arc::new(
        TaskValidationService::new(
            Arc::clone(&dac_client),
            Arc::clone(&ipfs_adapter),
            config.node_pubkey(),
        )
    );
    let agent_service = Arc::new(
        AgentService::new(
            Arc::clone(&dac_client),
            config.node_pubkey(),
        )
    );
    
    let shutdown_token = CancellationToken::new();
    
    let task_execution_handle = tokio::spawn({
        let token = shutdown_token.clone();
        let service = Arc::clone(&task_execution_service);
        async move {
            if let Err(e) = service.run_task_execution_loop(token).await {
                eprintln!("Task execution loop error: {}", e);
            }
        }
    });

    let task_validation_handle = tokio::spawn({
        let token = shutdown_token.clone();
        let service = Arc::clone(&task_validation_service);
        async move {
            if let Err(e) = service.run_task_validation_loop(token).await {
                eprintln!("Task validation loop error: {}", e);
            }
        }
    });

    let agent_validation_handle = tokio::spawn({
        let token = shutdown_token.clone();
        let service = Arc::clone(&agent_service);
        async move {
            if let Err(e) = service.run_agent_validation_loop(token).await {
                eprintln!("Agent validation loop error: {}", e);
            }
        }
    });

    println!("Public node ready. Monitoring for tasks to execute, validate, and agents to validate...");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Shutdown signal received. Stopping all services...");
        }
        _ = task_execution_handle => {
            println!("Task execution task ended unexpectedly");
        }
        _ = task_validation_handle => {
            println!("Task validation task ended unexpectedly");
        }
        _ = agent_validation_handle => {
            println!("Agent validation task ended unexpectedly");
        }
    }

    shutdown_token.cancel();
    // TODO: need to understand the best practices to wait for the tasks to complete

    println!("All services shut down cleanly.");
    Ok(())
}

async fn startup_checks(config: &Config, solana_adapter: &SolanaAdapter) -> Result<()> {
    let balance = solana_adapter.get_balance(&config.node_pubkey()).await?;
    println!("Balance: {} lamports ({} SOL)", balance, balance as f64 / 1_000_000_000.0);
    
    if balance < 1_000_000 {
        anyhow::bail!(
            "Node requires at least 0.001 SOL. Current balance: {} lamports. Please fund: {}",
            balance,
            config.node_pubkey()
        );
    }

    Ok(())
}
