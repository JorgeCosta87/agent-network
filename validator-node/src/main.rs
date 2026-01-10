mod config;
mod services;
mod utils;

use std::sync::Arc;
use anyhow::Result;
use config::Config;
use dac_sdk::SolanaAdapter;
use ipfs_adapter::IpfsClient;
use services::{TeeService, ValidatorService};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;

    println!("Starting DAC Validator Node...");
    println!("Node pubkey: {}", config.node_pubkey());
    println!("RPC URL: {}", config.rpc_url);
    println!("RPC WebSocket URL: {}", config.rpc_websocket_url);

    let solana_adapter = Arc::new(SolanaAdapter::new(
        config.rpc_url.clone(),
        config.rpc_websocket_url.clone(),
        config.node_keypair.insecure_clone()
    ).await);

    startup_checks(&config, &solana_adapter).await?;

    let ipfs_client = Arc::new(IpfsClient::new(&config.ipfs_api_url));
    println!("IPFS client initialized: {}", config.ipfs_api_url);

    let tee_service = Arc::new(TeeService::new());
    tee_service.initialize()?;

    let validator_service = Arc::new(ValidatorService::new(
        Arc::clone(&solana_adapter),
        Arc::clone(&ipfs_client),
        Arc::clone(&tee_service),
        config.network_authority,
        config.node_pubkey(),
    ));

    let node_pubkey = config.node_pubkey();
    
    // Try to claim validator node, or monitor until we can claim it
    let is_validator_claimed = validator_service.handle_validator_claim(&node_pubkey).await?;
    
    if !is_validator_claimed {
        println!("Validator node not claimed. Exiting. Please register the node on chain first.");
        return Ok(());
    }
    

    let shutdown_token = CancellationToken::new();
    
    // Spawn compute node validation loop
    let compute_validation_handle = tokio::spawn({
        let token = shutdown_token.clone();
        let service = Arc::clone(&validator_service);
        async move {
            if let Err(e) = service.run_validation_loop(token).await {
                eprintln!("Compute validation loop error: {}", e);
            }
        }
    });

    // Spawn agent validation loop
    let agent_validation_handle = tokio::spawn({
        let token = shutdown_token.clone();
        let service = Arc::clone(&validator_service);
        async move {
            if let Err(e) = service.run_agent_validation_loop(token).await {
                eprintln!("Agent validation loop error: {}", e);
            }
        }
    });

    // Spawn task validation loop
    let task_validation_handle = tokio::spawn({
        let token = shutdown_token.clone();
        let service = Arc::clone(&validator_service);
        async move {
            if let Err(e) = service.run_task_validation_loop(token).await {
                eprintln!("Task validation loop error: {}", e);
            }
        }
    });

    println!("Validator node ready. Monitoring for compute nodes, agents, and tasks to validate...");
    println!("Press Ctrl+C to shutdown.");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Shutdown signal received. Stopping all validation loops...");
        }
        _ = compute_validation_handle => {
            println!("Compute validation task ended unexpectedly");
        }
        _ = agent_validation_handle => {
            println!("Agent validation task ended unexpectedly");
        }
        _ = task_validation_handle => {
            println!("Task validation task ended unexpectedly");
        }
    }

    shutdown_token.cancel();
    //TODO: need to undersant the best pratices to wait for the tasks to complete

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
