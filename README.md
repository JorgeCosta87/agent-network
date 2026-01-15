# DAC Agent Network

Decentralized network for verifiable AI agent execution on Solana.

## Quick Start

```bash
# 1. Start IPFS
make ipfs-up

# 2. Configure environment
cp .env.example .env

# 3. Generate keypairs (from project root)
./scripts/generate_keypair.sh [num_public_nodes] [num_confidential_nodes]
# Example: ./scripts/generate_keypair.sh 2 1

# 4. Run public node
cd agent-network
cargo run --bin public-node ../keypairs/public-node-1-keypair.json

# 5. Run confidential node
cd confidential-node && cargo run
```

**Note**: Paths are resolved relative to your current working directory. Use `../keypairs/...` from `agent-network/` or `keypairs/...` from project root.

## Commands

```bash
make help          # Show all commands
make ipfs-up       # Start IPFS
make ipfs-down     # Stop IPFS
make check         # Check code
make build         # Build binaries
make test          # Run tests
```


## Components

- **public-node**: Validates and executes tasks (public execution)
- **confidential-node**: Validates and executes tasks with TEE protection (confidential execution)
- **dac-sdk**: Solana program client library
- **ipfs-adapter**: IPFS client wrapper
- **solana-adapter**: Solana RPC wrapper

## Docs

- [Docker Setup](README_DOCKER.md) | [Design](../docs/design.md) | [IPFS Architecture](docs/IPFS_ARCHITECTURE.md) | [Configs](../docs/configs.yaml)
