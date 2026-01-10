use anyhow::{Context, Result};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::path::PathBuf;

pub struct Config {
    pub rpc_url: String,
    pub rpc_websocket_url: String,
    pub node_keypair: Keypair,
    pub network_authority: Pubkey,
    pub ipfs_api_url: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        dotenvy::dotenv().context("Failed to load .env file")?;

        let rpc_url = std::env::var("RPC_URL")
            .context("RPC_URL environment variable not set")?;
        let rpc_websocket_url = std::env::var("RPC_WEBSOCKET_URL")
            .context("RPC_WEBSOCKET_URL environment variable not set")?;

        let keypair_path =
            std::env::var("VALIDATOR_NODE_KEYPAIR_PATH")
                .context("VALIDATOR_NODE_KEYPAIR_PATH environment variable not set")?;
        let keypair_path = PathBuf::from(keypair_path);

        let keypair = Self::load_keypair_from_file(&keypair_path)
            .context(format!("Failed to load keypair from {}", keypair_path.display()))?;

        let network_authority = std::env::var("NETWORK_AUTHORITY")
            .context("NETWORK_AUTHORITY environment variable not set")?;
        let network_authority = network_authority.parse::<Pubkey>()
            .map_err(|_| anyhow::anyhow!("Failed to parse NETWORK_AUTHORITY as Pubkey"))?;

        let ipfs_api_url = std::env::var("IPFS_API_URL")
            .unwrap_or_else(|_| "http://localhost:5001".to_string());

        Ok(Config {
            rpc_url,
            rpc_websocket_url,
            node_keypair: keypair,
            network_authority,
            ipfs_api_url,
        })
    }

    fn load_keypair_from_file(path: &PathBuf) -> Result<Keypair> {
        let keypair_json = std::fs::read_to_string(path).context("Failed to read keypair file")?;

        let keypair_bytes: Vec<u8> =
            serde_json::from_str(&keypair_json).context("Failed to parse keypair JSON")?;

        if keypair_bytes.len() != 64 {
            anyhow::bail!(
                "Invalid keypair length: expected 64 bytes, got {}",
                keypair_bytes.len()
            );
        }

        let mut secret_key = [0u8; 32];
        secret_key.copy_from_slice(&keypair_bytes[..32]);
        let keypair = Keypair::new_from_array(secret_key);

        Ok(keypair)
    }

    pub fn node_pubkey(&self) -> Pubkey {
        self.node_keypair.pubkey()
    }
}
