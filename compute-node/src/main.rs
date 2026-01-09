mod config;
mod services;

use std::sync::Arc;
use anyhow::Result;
use config::Config;
use dac_sdk::SolanaAdapter;
use services::{NodeRegistryService, AgentService};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;

    println!("Starting DAC Compute Node...");
    println!("Node pubkey: {}", config.node_pubkey());
    println!("RPC URL: {}", config.rpc_url);
    println!("RPC WebSocket URL: {}", config.rpc_websocket_url);

    let solana_adapter = Arc::new(SolanaAdapter::new(
        config.rpc_url.clone(),
        config.rpc_websocket_url.clone(),
        config.node_keypair.insecure_clone()
    ).await);

    startup_checks(&config, &solana_adapter).await?;

    let node_registry_service = NodeRegistryService::new(Arc::clone(&solana_adapter));
    node_registry_service.check_and_claim_pending_node(&config.node_pubkey()).await?;
    
    let mut agent_service = AgentService::new(Arc::clone(&solana_adapter));
    agent_service.monitor_and_register_agents(&config.node_pubkey()).await?;

    tokio::signal::ctrl_c().await?;
    println!("Shutting down...");

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
