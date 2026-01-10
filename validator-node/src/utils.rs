use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::Signer;
use solana_ed25519_program::new_ed25519_instruction_with_signature;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer as SolanaSigner},
};

#[derive(BorshSerialize, BorshDeserialize)]
pub struct ValidateComputeNodeMessage {
    pub compute_node_pubkey: Pubkey,
    pub approved: bool,
}

pub fn create_ed25519_instruction_with_signature(
    message: &[u8],
    key_pair: &Keypair,
) -> Instruction {
    let message_data = message.to_vec();

    let tee_keypair_bytes = key_pair.to_bytes();
    let mut tee_secret_bytes = [0u8; 32];
    tee_secret_bytes.copy_from_slice(&tee_keypair_bytes[..32]);
    let tee_secret_key = ed25519_dalek::SigningKey::from_bytes(&tee_secret_bytes);
    let signature = tee_secret_key.sign(&message_data);

    let tee_pubkey = key_pair.pubkey();
    let mut tee_pubkey_bytes = [0u8; 32];
    tee_pubkey_bytes.copy_from_slice(tee_pubkey.as_ref());
    let signature_bytes: [u8; 64] = signature.to_bytes();

    new_ed25519_instruction_with_signature(&message_data, &signature_bytes, &tee_pubkey_bytes)
}