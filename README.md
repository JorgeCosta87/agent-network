# DAC Agent Network

Decentralized network for verifiable AI agent execution on Solana.

## Quick Start

### 1. Start IPFS

```bash
make ipfs-up
```

### 2. Configure Environment

```bash
cp .env.example .env
# Edit .env with your configuration
```

### 3. Run Validator Node

```bash
make validator
```

### 4. Run Compute Node (in another terminal)

```bash
make compute
```

## Docker Commands

```bash
make help           # Show all commands
make ipfs-up        # Start IPFS
make ipfs-status    # Check IPFS status
make ipfs-logs      # View logs
make ipfs-test      # Test connection
make ipfs-down      # Stop IPFS
make ipfs-clean     # Remove all data
```

## Development

```bash
make check      # Check code
make build      # Build binaries
make test       # Run tests
make clean      # Clean artifacts
```

## Architecture

```
┌──────────────┐
│ Validator    │──┐
│ Node (TEE)   │  │
└──────────────┘  │
                  │    ┌──────────┐      ┌──────────┐
┌──────────────┐  ├───▶│   IPFS   │◀────▶│  Solana  │
│  Compute     │  │    │  Docker  │      │  Network │
│  Node (LLM)  │──┘    └──────────┘      └──────────┘
└──────────────┘

```

## Components

- **validator-node**: Validates compute nodes, agents, and tasks (TEE-enabled)
- **compute-node**: Executes LLM tasks
- **dac-sdk**: Solana program client library
- **ipfs-adapter**: IPFS client wrapper
- **solana-adapter**: Solana RPC wrapper

## Documentation

- [Docker Setup](README_DOCKER.md) - IPFS Docker configuration
- [Design](../docs/design.md) - System design
- [IPFS Architecture](docs/IPFS_ARCHITECTURE.md) - IPFS data flow
- [Configs](../docs/configs.yaml) - Configuration schemas
