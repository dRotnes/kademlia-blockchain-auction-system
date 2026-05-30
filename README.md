# Kademlia Blockchain Auction System

This is a Rust prototype of a non-permissioned auction ledger built on top of a secure Kademlia-style peer-to-peer network. It was originally started for the System and Data Security 2024/2025 assignment and has been cleaned up into a portfolio-friendly project that can be built, tested, and demonstrated locally.

The implementation focuses on three assignment themes:

- A public ledger model with hash-addressed blocks and transactions.
- A secure P2P layer using signed gRPC messages, node IDs derived from public keys, Kademlia routing tables, bootstrap challenges, `STORE`, `FIND_NODE`, and `FIND_VALUE`.
- Auction data that can be serialized over protobuf, stored by DHT key, and retrieved by key.

## Current Status

Implemented:

- Rust gRPC service generated from `proto/kademlia.proto`.
- Authenticated message wrapper with RSA signatures.
- Sender identity validation by hashing the sender public key.
- Bootstrap proof-of-work challenge before a peer joins through the bootstrap node.
- Kademlia routing table with k-buckets and XOR-distance lookup.
- Auction creation, deterministic auction keys, bid validation, protobuf conversion, local storage, and DHT-style `FIND_VALUE` responses.
- One-shot demo commands for creating auctions, finding auctions, and placing bids.
- JSON persistence for auctions and local ledger blocks under `data/<port>/`.
- Configurable consensus mode: proof-of-work style local commits or proof-of-reputation gating for winner-confirmation blocks.
- Optional automated auction creation, random bidding, automatic auction closure, and winner-confirmation blocks.
- Self-contained protobuf build using vendored `protoc`.
- Unit tests for command parsing, signed messages, auction rules/serialization, persistence, proof-of-reputation, node auction storage, and block hashing.

Still prototype-level:

- Consensus is represented by block and proof-of-work primitives, but there is no full replicated chain synchronization or fork-choice implementation yet.
- Auction bidding supports simple highest-bid validation, but there is no close-auction workflow or winner settlement yet.
- Ledger blocks are local to each node; full block gossip and fork-choice are still future work.
- The application has a small demo CLI, not a polished end-user CLI or web UI.

## Architecture

```text
src/
  main.rs                 process bootstrap: config, keys, node, server/client
  command.rs              minimal parser for one-shot demo commands
  consensus.rs            proof-of-work/proof-of-reputation commit checks
  storage.rs              JSON persistence for auctions and ledger blocks
  auction.rs              auction and bid domain objects plus protobuf conversion
  node.rs                 local node identity, routing table, and auction store
  g_rpc/
    server.rs             Kademlia gRPC server implementation
    client.rs             bootstrap, challenge, store, find-node, find-value client calls
  routing/
    kbucket.rs            single Kademlia bucket
    routing_table.rs      XOR-distance bucket selection and closest-node queries
  blockchain/
    address.rs            256-bit address wrapper
    block.rs              hash-addressed block structure
    transaction.rs        simple transaction model
  utils/
    config.rs             environment and CLI config
    crypto_own.rs         hashing, RSA signatures, authenticated message handling
    execution.rs          server/client thread runner
```

## Requirements

- Rust stable toolchain.
- OpenSSL development libraries available to the Rust `openssl` crate.

You do not need to install `protoc` manually; the build uses `protoc-bin-vendored`.

## Configuration

Runtime configuration is read from `.env` plus the required `--port` CLI argument.

Important values:

```env
BOOTSTRAP_PEER_IP='127.0.0.1'
BOOTSTRAP_PEER_PORT=8000
MAX_N_KBUCKET_ENTRIES=2
PEER_SYNC_MS=10000
CHALLENGE_DIFFICULTY=20
K_VALUE=3
CONSENSUS_MODE='pow'
REPUTATION_THRESHOLD=1
AUTOMATION_ENABLED=false
AUTOMATION_INTERVAL_MS=5000
AUCTION_DURATION_MS=30000
```

Each node stores its identity under:

```text
keys/<port>/private_key.pem
keys/<port>/public_key.pem
```

Keys are generated automatically on first run and are ignored by git.

Persistent node data is stored under:

```text
data/<port>/auctions.json
data/<port>/ledger.json
```

For quick local demos, consider lowering `CHALLENGE_DIFFICULTY` to `5` or `8`. The checked-in value `20` is intentionally more expensive.

## Run Tests

```bash
cargo test
```

Expected result:

```text
running 14 tests
test result: ok. 14 passed
```

## Run A Local Network

Open multiple terminals from the repository root.

Terminal 1, bootstrap node:

```bash
cargo run -- --port 8000
```

Terminal 2, peer node:

```bash
cargo run -- --port 8001
```

Terminal 3, another peer:

```bash
cargo run -- --port 8002
```

The bootstrap node skips bootstrapping itself. Other peers request a challenge from the bootstrap node, solve it, insert the bootstrap node into their routing table, run `FIND_NODE`, and then start.

## Use The Demo Commands

Keep the bootstrap daemon running:

```bash
cargo run -- --port 8000
```

Then run one-shot commands from other terminals. These command processes solve the bootstrap challenge, perform the requested action, print the result, and exit. They do not stay in the routing table as long-running peers.

Create an auction:

```bash
cargo run -- --port 8001 create-auction "Vintage keyboard" 50
```

Find an auction by key:

```bash
cargo run -- --port 8002 find-auction <auction_key>
```

Place a bid:

```bash
cargo run -- --port 8002 bid <auction_key> 75
```

Bids must be strictly higher than the current auction price. The current price is the initial value until the first valid bid, then the highest bid amount.

## Start An Automated Network

The helper script starts several long-running nodes with automation enabled:

```bash
scripts/start_network.sh
```

You can pass custom ports:

```bash
scripts/start_network.sh 8000 8001 8002 8003
```

By default the script sets:

```env
AUTOMATION_ENABLED=true
AUTOMATION_INTERVAL_MS=3000
AUCTION_DURATION_MS=15000
CONSENSUS_MODE=pow
```

Automated nodes randomly create auctions from a built-in item list, bid on open auctions they did not create, close expired auctions, and append local ledger blocks confirming the winner and winning bid.

To exercise proof-of-reputation gating:

```bash
CONSENSUS_MODE=por REPUTATION_THRESHOLD=1 scripts/start_network.sh
```

In proof-of-reputation mode, a node only commits winner-confirmation blocks after its address has enough reputation according to previous auction-winner ledger entries. This is intentionally a prototype rule, but it makes the consensus mode configurable and testable.

## Protocol Summary

All RPCs accept and return `AuthenticatedMessage`:

- `sender`: node ID, IP, and port.
- `public_key`: DER-encoded public key.
- `signature`: RSA/SHA-256 signature over the protobuf payload.
- `payload`: encoded request or response message.

The receiver verifies:

1. The signature is valid for the payload and public key.
2. The sender ID equals `sha256(public_key)`.
3. The payload decodes into the expected RPC type.

Supported RPCs:

- `Ping`
- `Bootstrap`
- `ChallengeResolution`
- `Store`
- `FindNode`
- `FindValue`
