use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use dac_client::programs::DAC_ID;

pub fn derive_network_config_pda() -> Result<(Pubkey, u8)> {
    Pubkey::try_find_program_address(
        &[b"dac_network_config"],
        &DAC_ID,
    )
    .ok_or_else(|| anyhow::anyhow!("Failed to derive network config PDA"))
}

pub fn derive_node_info_pda(node_pubkey: &Pubkey) -> Result<(Pubkey, u8)> {
    Pubkey::try_find_program_address(&[b"node_info", node_pubkey.as_ref()], &DAC_ID)
        .ok_or_else(|| anyhow::anyhow!("Failed to derive node info PDA"))
}

pub fn derive_agent_pda(network_config: &Pubkey, agent_slot_id: u64) -> Result<(Pubkey, u8)> {
    Pubkey::try_find_program_address(
        &[b"agent", network_config.as_ref(), &agent_slot_id.to_le_bytes()],
        &DAC_ID,
    )
    .ok_or_else(|| anyhow::anyhow!("Failed to derive agent PDA"))
}

pub fn derive_goal_pda(network_config: &Pubkey, goal_slot_id: u64) -> Result<(Pubkey, u8)> {
    Pubkey::try_find_program_address(
        &[b"goal", network_config.as_ref(), &goal_slot_id.to_le_bytes()],
        &DAC_ID,
    )
    .ok_or_else(|| anyhow::anyhow!("Failed to derive goal PDA"))
}

pub fn derive_task_pda(network_config: &Pubkey, task_slot_id: u64) -> Result<(Pubkey, u8)> {
    Pubkey::try_find_program_address(
        &[b"task", network_config.as_ref(), &task_slot_id.to_le_bytes()],
        &DAC_ID,
    )
    .ok_or_else(|| anyhow::anyhow!("Failed to derive task PDA"))
}

pub fn derive_node_treasury_pda(node_info: &Pubkey) -> Result<(Pubkey, u8)> {
    Pubkey::try_find_program_address(&[b"node_treasury", node_info.as_ref()], &DAC_ID)
        .ok_or_else(|| anyhow::anyhow!("Failed to derive node treasury PDA"))
}

pub fn derive_goal_vault_pda(goal: &Pubkey) -> Result<(Pubkey, u8)> {
    Pubkey::try_find_program_address(&[b"goal_vault", goal.as_ref()], &DAC_ID)
        .ok_or_else(|| anyhow::anyhow!("Failed to derive goal vault PDA"))
}

pub fn derive_contribution_pda(goal: &Pubkey, contributor: &Pubkey) -> Result<(Pubkey, u8)> {
    Pubkey::try_find_program_address(
        &[b"contribution", goal.as_ref(), contributor.as_ref()],
        &DAC_ID,
    )
    .ok_or_else(|| anyhow::anyhow!("Failed to derive contribution PDA"))
}
