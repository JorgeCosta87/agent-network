#!/bin/bash

# Generate keypairs for compute node and validator node
# This script generates Solana keypairs and saves them to the agent-network root folder

set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Get the agent-network root directory (parent of scripts)
AGENT_NETWORK_ROOT="$(dirname "$SCRIPT_DIR")"

# Keypair paths relative to agent-network root
COMPUTE_KEYPAIR_PATH="$AGENT_NETWORK_ROOT/compute-node-keypair.json"
VALIDATOR_KEYPAIR_PATH="$AGENT_NETWORK_ROOT/validator-node-keypair.json"
KEYPAIRS_JSON_PATH="$AGENT_NETWORK_ROOT/keypairs.json"

echo "Generating keypairs for compute node and validator node..."
echo "Agent network root: $AGENT_NETWORK_ROOT"
echo ""

# Generate compute node keypair
echo "Generating compute node keypair..."
solana-keygen new --outfile "$COMPUTE_KEYPAIR_PATH" --no-bip39-passphrase --force
COMPUTE_PUBKEY=$(solana-keygen pubkey "$COMPUTE_KEYPAIR_PATH")
echo "Compute node pubkey: $COMPUTE_PUBKEY"
echo ""

# Generate validator node keypair
echo "Generating validator node keypair..."
solana-keygen new --outfile "$VALIDATOR_KEYPAIR_PATH" --no-bip39-passphrase --force
VALIDATOR_PUBKEY=$(solana-keygen pubkey "$VALIDATOR_KEYPAIR_PATH")
echo "Validator node pubkey: $VALIDATOR_PUBKEY"
echo ""

# Create JSON file with pubkeys
cat > "$KEYPAIRS_JSON_PATH" << EOF
{
  "compute_node": {
    "pubkey": "$COMPUTE_PUBKEY",
    "keypair_path": "compute-node-keypair.json"
  },
  "validator_node": {
    "pubkey": "$VALIDATOR_PUBKEY",
    "keypair_path": "validator-node-keypair.json"
  }
}
EOF

echo "Generated keypairs:"
echo "  Compute node: $COMPUTE_KEYPAIR_PATH"
echo "  Validator node: $VALIDATOR_KEYPAIR_PATH"
echo "  Keypairs JSON: $KEYPAIRS_JSON_PATH"
echo ""
echo "Keypair addresses saved to: $KEYPAIRS_JSON_PATH"
echo ""
echo "The keypair files contain both public and private keys."

