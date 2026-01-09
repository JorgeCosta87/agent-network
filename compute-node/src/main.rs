mod config;
mod services;

use std::sync::Arc;
use anyhow::Result;
use config::Config;
use dac_sdk::SolanaAdapter;
use services::AgentService;
use services::NodeClaimService;

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

    let node_claim_service = Arc::new(NodeClaimService::new(Arc::clone(&solana_adapter)));
    let agent_service = Arc::new(tokio::sync::Mutex::new(AgentService::new(Arc::clone(&solana_adapter))));
    
    let node_pubkey = config.node_pubkey();
    
    let is_node_claimed = node_claim_service.handle_node_claim(&node_pubkey).await?;
    
    // Only start services if node is successfully claimed
    if !is_node_claimed {
        println!("Node not claimed. Exiting. Please register the node first.");
        return Ok(());
    }
    
    println!("Node claimed. Starting agent monitoring...");
    
    // Start agent monitoring now that node is claimed
    let agent_service_clone = Arc::clone(&agent_service);
    let node_pubkey_clone = node_pubkey;
    let agent_monitor_handle = tokio::spawn(async move {
        let mut service = agent_service_clone.lock().await;
        if let Err(e) = service.monitor_and_register_agents(&node_pubkey_clone).await {
            eprintln!("Agent monitoring error: {}", e);
        }
    });
    
    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    println!("Shutting down...");
    
    // Cancel monitoring tasks
    agent_monitor_handle.abort();

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
